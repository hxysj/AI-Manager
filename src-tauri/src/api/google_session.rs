use crate::{
    api::runtime_provider::{decrypt_provider_key, encrypt_provider_key},
    core::{database, error::ManagerError, paths::AppPaths},
};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::collections::VecDeque;

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_SESSIONS: usize = 128;
const TTL_SECONDS: i64 = 24 * 60 * 60;

// store:false 只进入当前 WebSocket 的缓存，断开连接即释放，不跨账号、连接共享。
#[derive(Default)]
pub(crate) struct ConnectionSessions {
    entries: VecDeque<(String, Value, usize)>,
    bytes: usize,
}

impl ConnectionSessions {
    pub fn get(&self, id: &str) -> Option<Value> {
        self.entries
            .iter()
            .find(|entry| entry.0 == id)
            .map(|entry| entry.1.clone())
    }

    pub fn insert(&mut self, id: &str, session: Value) -> Result<(), String> {
        let size = session.to_string().len();
        if size > MAX_BYTES {
            return Err("会话超过本地缓存容量，请压缩上下文后重试".into());
        }
        while self.entries.len() >= MAX_SESSIONS || self.bytes + size > MAX_BYTES {
            if let Some(entry) = self.entries.pop_front() {
                self.bytes -= entry.2;
            }
        }
        self.bytes += size;
        self.entries.push_back((id.to_owned(), session, size));
        Ok(())
    }
}

fn open(paths: &AppPaths) -> Result<rusqlite::Connection, ManagerError> {
    let connection = database::open(paths)?;
    // 运行态缓存不在数据库的业务备份白名单中；内容沿用本地凭据加密方式。
    connection.execute_batch("CREATE TABLE IF NOT EXISTS google_response_sessions (
        provider_id TEXT NOT NULL, response_id TEXT NOT NULL, created_at INTEGER NOT NULL,
        payload TEXT NOT NULL, size INTEGER NOT NULL, PRIMARY KEY(provider_id, response_id)
    ); CREATE INDEX IF NOT EXISTS google_response_sessions_age ON google_response_sessions(created_at);")?;
    connection.execute(
        "DELETE FROM google_response_sessions WHERE created_at < ?1",
        [chrono::Utc::now().timestamp() - TTL_SECONDS],
    )?;
    Ok(connection)
}

pub(crate) fn load(
    paths: &AppPaths,
    provider: &str,
    id: &str,
) -> Result<Option<Value>, ManagerError> {
    let connection = open(paths)?;
    let value: Option<String> = connection
        .query_row(
            "SELECT payload FROM google_response_sessions WHERE provider_id=?1 AND response_id=?2",
            params![provider, id],
            |row| row.get(0),
        )
        .optional()?;
    value
        .map(|text| Ok(serde_json::from_str(&decrypt_provider_key(&text)?)?))
        .transpose()
}

pub(crate) fn save(
    paths: &AppPaths,
    provider: &str,
    id: &str,
    session: &Value,
) -> Result<(), ManagerError> {
    let payload = encrypt_provider_key(&session.to_string())?;
    if payload.len() > MAX_BYTES {
        return Err(ManagerError::System(
            "会话超过本地缓存容量，请压缩上下文后重试".into(),
        ));
    }
    let mut connection = open(paths)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT OR REPLACE INTO google_response_sessions VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            provider,
            id,
            chrono::Utc::now().timestamp(),
            payload,
            payload.len() as i64
        ],
    )?;
    loop {
        let (count, bytes): (i64, i64) = transaction.query_row(
            "SELECT count(*), coalesce(sum(size), 0) FROM google_response_sessions WHERE provider_id=?1",
            [provider], |row| Ok((row.get(0)?, row.get(1)?)))?;
        if count <= MAX_SESSIONS as i64 && bytes <= MAX_BYTES as i64 {
            break;
        }
        transaction.execute("DELETE FROM google_response_sessions WHERE rowid IN (
            SELECT rowid FROM google_response_sessions WHERE provider_id=?1 ORDER BY created_at, rowid LIMIT 1)", [provider])?;
    }
    transaction.commit()?;
    Ok(())
}

pub(crate) fn clear(paths: &AppPaths, provider: &str) -> Result<(), ManagerError> {
    open(paths)?.execute(
        "DELETE FROM google_response_sessions WHERE provider_id=?1",
        [provider],
    )?;
    Ok(())
}

pub(crate) fn merge(body: &mut Value, parent: Option<Value>) -> Result<(), String> {
    let mut input = match &body["input"] {
        Value::String(text) => vec![json!({"role": "user", "content": text})],
        Value::Array(items) => items.clone(),
        Value::Null => Vec::new(),
        _ => return Err("Responses input 必须是文本或消息数组".into()),
    };
    if let Some(parent) = parent {
        let mut history = parent["input"]
            .as_array()
            .cloned()
            .ok_or("本地会话数据无效")?;
        history.append(&mut input);
        input = history;
        if body["model"].as_str().is_none_or(str::is_empty) {
            body["model"] = parent["model"].clone();
        }
        // Responses 的 instructions 不继承，调用方每轮提供本轮规则。
    }
    body["input"] = json!(input);
    body.as_object_mut()
        .ok_or("请求必须是 JSON 对象")?
        .remove("previous_response_id");
    if body["input"].to_string().len() > MAX_BYTES / 2 {
        return Err("会话上下文超过 32 MiB，请压缩上下文后重试".into());
    }
    Ok(())
}

pub(crate) fn snapshot(body: &Value, response: &Value) -> Value {
    let mut input = body["input"].as_array().cloned().unwrap_or_default();
    input.extend(response["output"].as_array().into_iter().flatten().cloned());
    json!({"model": body["model"], "input": input})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_sessions_persist_encrypted_isolate_accounts_and_preserve_branches() {
        let root = std::env::temp_dir().join(format!("google-session-{}", uuid::Uuid::new_v4()));
        let paths = crate::core::paths::resolve_app_paths(&root);
        let session =
            json!({"model": "gemini", "input": [{"role": "user", "content": "private-content"}]});
        save(&paths, "account-a", "resp_parent", &session).unwrap();
        assert!(load(&paths, "account-b", "resp_parent").unwrap().is_none());
        for text in ["分支一", "分支二"] {
            let mut body = json!({"input": text, "previous_response_id": "resp_parent"});
            merge(&mut body, load(&paths, "account-a", "resp_parent").unwrap()).unwrap();
            assert_eq!(body["input"].as_array().unwrap().len(), 2);
            assert_eq!(body["input"][1]["content"], text);
            assert!(body.get("instructions").is_none());
            assert!(body.get("previous_response_id").is_none());
        }
        let connection = open(&paths).unwrap();
        let raw: String = connection
            .query_row("SELECT payload FROM google_response_sessions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(!raw.contains("private-content"));
        connection
            .execute("UPDATE google_response_sessions SET created_at=0", [])
            .unwrap();
        assert!(load(&paths, "account-a", "resp_parent").unwrap().is_none());
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn google_connection_sessions_are_bounded_and_do_not_cross_connections() {
        let mut cache = ConnectionSessions::default();
        for i in 0..=MAX_SESSIONS {
            cache
                .insert(&format!("resp_{i}"), json!({"input": []}))
                .unwrap();
        }
        assert!(cache.get("resp_0").is_none());
        assert!(cache.get("resp_1").is_some());
        assert!(ConnectionSessions::default().get("resp_1").is_none());
    }
}
