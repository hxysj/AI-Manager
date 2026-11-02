use super::*;
use rusqlite::params_from_iter;
use rusqlite::types::Value as SqlValue;
use serde_json::{json, Map, Value};

pub struct RestoreRowDifference {
    pub table: String,
    pub row_key: String,
    pub current: Option<Value>,
    pub backup: Value,
}

pub fn row_choice_key(table: &str, row_key: &str) -> String {
    format!("database-row:storage/ai-manager.db:{table}:{row_key}")
}

pub fn preview_rows(
    paths: &AppPaths,
    content: &[u8],
) -> Result<Vec<RestoreRowDifference>, ManagerError> {
    with_restore_snapshot(paths, content, |snapshot_path| {
        let connection = open_restore_connection(paths, snapshot_path)?;
        let mut differences = Vec::new();
        for table in read_restorable_tables(&connection)? {
            let columns = read_common_columns(&connection, &table)?;
            let key_column = primary_key(&connection, &table)?;
            let mut current = read_rows(&connection, "main", &table, &columns, &key_column)?;
            let mut backup = read_rows(&connection, "restore_source", &table, &columns, &key_column)?
                .into_iter()
                .collect::<Vec<_>>();
            backup.sort_by(|left, right| {
                left.1
                    .get("sort_order")
                    .and_then(Value::as_i64)
                    .cmp(&right.1.get("sort_order").and_then(Value::as_i64))
                    .then(left.0.cmp(&right.0))
            });
            for (row_key, row) in backup {
                let existing = existing_row(&current, &table, &row_key, &row);
                let backup = comparable_row(&connection, &table, &row)?;
                let current_value = existing
                    .map(|(_, row)| row)
                    .map(|row| comparable_row(&connection, &table, row))
                    .transpose()?;
                if current_value.as_ref() != Some(&backup) {
                    differences.push(RestoreRowDifference {
                        table: table.clone(),
                        row_key: row_key.clone(),
                        current: current_value,
                        backup,
                    });
                }
                if existing.is_none() {
                    current.insert(row_key, row);
                }
            }
        }
        differences.sort_by(|left, right| {
            (&left.table, &left.row_key).cmp(&(&right.table, &right.row_key))
        });
        Ok(differences)
    })
}

fn existing_row<'a>(
    current: &'a HashMap<String, Map<String, Value>>,
    table: &str,
    row_key: &str,
    row: &Map<String, Value>,
) -> Option<(&'a String, &'a Map<String, Value>)> {
    current.get_key_value(row_key).or_else(|| {
        if table != "providers" || row.get("type") != Some(&json!("codex-account")) {
            return None;
        }
        let account = Value::Object(row.clone());
        current
            .iter()
            .filter(|(_, candidate)| {
                candidate.get("type") == row.get("type")
                    && crate::core::provider_store::codex_accounts_match(
                        &Value::Object((*candidate).clone()),
                        &account,
                    )
            })
            .min_by_key(|(key, candidate)| {
                (candidate.get("is_active").and_then(Value::as_i64) != Some(1), *key)
            })
    })
}

fn primary_key(connection: &Connection, table: &str) -> Result<String, ManagerError> {
    let mut statement = connection.prepare(&format!(
        "PRAGMA main.table_info({})",
        quote_identifier(table)
    ))?;
    let keys = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|(_, index)| *index > 0)
        .collect::<Vec<_>>();
    if keys.len() != 1 {
        return Err(ManagerError::System(format!(
            "数据表 {table} 缺少可合并的唯一标识。"
        )));
    }
    Ok(keys[0].0.clone())
}

fn read_rows(
    connection: &Connection,
    schema: &str,
    table: &str,
    columns: &[String],
    key_column: &str,
) -> Result<HashMap<String, Map<String, Value>>, ManagerError> {
    let sql = format!(
        "SELECT {} FROM {schema}.{} {}",
        columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", "),
        quote_identifier(table),
        restore_row_filter(table)
    );
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query([])?;
    let mut result = HashMap::new();
    while let Some(row) = rows.next()? {
        let mut value = Map::new();
        for (index, column) in columns.iter().enumerate() {
            let item = match row.get::<_, SqlValue>(index)? {
                SqlValue::Null => Value::Null,
                SqlValue::Integer(value) => json!(value),
                SqlValue::Real(value) => json!(value),
                SqlValue::Text(value) => json!(value),
                SqlValue::Blob(_) => {
                    return Err(ManagerError::System(format!(
                        "数据表 {table} 包含不支持的二进制字段。"
                    )))
                }
            };
            value.insert(column.clone(), item);
        }
        let key = value
            .get(key_column)
            .ok_or_else(|| ManagerError::System(format!("备份数据表 {table} 缺少主键。")))?;
        result.insert(
            key.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| key.to_string()),
            value,
        );
    }
    Ok(result)
}

fn comparable_row(
    _connection: &Connection,
    table: &str,
    row: &Map<String, Value>,
) -> Result<Value, ManagerError> {
    let mut value = row.clone();
    value.remove("sort_order");
    if table == "providers" {
        if value.get("type") == Some(&json!("codex-account")) {
            // 同一官方账号在不同设备上的本地主键不属于业务差异。
            value.remove("id");
        }
        for column in PROVIDER_LOCAL_COLUMNS.iter().copied().chain([
            "created_at",
            "updated_at",
            "last_refresh",
            "token_updated_at",
        ]) {
            value.remove(column);
        }
    }
    if table == "skills" {
        for column in SKILL_LOCAL_COLUMNS {
            value.remove(*column);
        }
    }
    if table == "skill_repositories" {
        for column in REPOSITORY_LOCAL_COLUMNS {
            value.remove(*column);
        }
    }
    if table == "rules" {
        for column in RULE_LOCAL_COLUMNS {
            value.remove(*column);
        }
    }
    if table.starts_with("skill") || table == "rules" {
        value.remove("created_at");
        value.remove("updated_at");
    }
    Ok(Value::Object(value))
}

pub(super) fn merge_table(
    transaction: &rusqlite::Transaction<'_>,
    table: &str,
) -> Result<(), ManagerError> {
    let columns = read_common_columns(transaction, table)?;
    let key_column = primary_key(transaction, table)?;
    let mut current = read_rows(transaction, "main", table, &columns, &key_column)?;
    let mut backup = read_rows(transaction, "restore_source", table, &columns, &key_column)?
        .into_iter()
        .collect::<Vec<_>>();
    backup.sort_by(|left, right| {
        left.1
            .get("sort_order")
            .and_then(Value::as_i64)
            .cmp(&right.1.get("sort_order").and_then(Value::as_i64))
            .then(left.0.cmp(&right.0))
    });
    let mut next_order = current
        .values()
        .filter_map(|row| row.get("sort_order").and_then(Value::as_i64))
        .max()
        .unwrap_or(-1)
        + 1;
    let quoted_columns = columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect::<Vec<_>>();
    let updates = quoted_columns
        .iter()
        .filter(|column| **column != quote_identifier(&key_column))
        .map(|column| format!("{column} = excluded.{column}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO main.{} ({}) VALUES ({}) ON CONFLICT({}) DO UPDATE SET {updates}",
        quote_identifier(table),
        quoted_columns.join(", "),
        vec!["?"; columns.len()].join(", "),
        quote_identifier(&key_column)
    );
    for (row_key, mut row) in backup {
        let existing = existing_row(&current, table, &row_key, &row);
        let target_key = existing
            .map(|(key, _)| key.clone())
            .unwrap_or_else(|| row_key.clone());
        let existing = existing.map(|(_, row)| row);
        let selected = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM restore_choices WHERE choice_key IN (?1, '*'))",
            [row_choice_key(table, &row_key)],
            |row| row.get::<_, bool>(0),
        )?;
        if let Some(existing) = existing {
            if !selected
                || comparable_row(transaction, table, existing)?
                    == comparable_row(transaction, table, &row)?
            {
                continue;
            }
            if let Some(order) = existing.get("sort_order") {
                row.insert("sort_order".into(), order.clone());
            }
        } else if row.contains_key("sort_order") {
            row.insert("sort_order".into(), json!(next_order));
            next_order += 1;
        }
        // 关联历史和本机激活态继续使用本机主键，备份只更新业务字段。
        row.insert(key_column.clone(), json!(target_key));
        if table == "providers" {
            // 在写入前恢复本机状态，防止备份中的激活记录触发唯一约束。
            let same_scope = existing.is_some_and(|old| {
                old.get("cli") == row.get("cli") && old.get("type") == row.get("type")
            });
            for column in PROVIDER_LOCAL_COLUMNS {
                let keep = same_scope || ["enabled", "oauth_credentials", "quota"].contains(column);
                let value = if keep {
                    existing.and_then(|old| old.get(*column)).cloned()
                } else {
                    None
                };
                row.insert(
                    (*column).into(),
                    value.unwrap_or_else(|| provider_local_default(column)),
                );
            }
        }
        if table == "skill_repositories" {
            let same_source = existing.is_some_and(|old| {
                [
                    "type",
                    "source",
                    "owner",
                    "repository",
                    "branch",
                    "root_path",
                ]
                .iter()
                .all(|column| old.get(*column) == row.get(*column))
            });
            for column in REPOSITORY_LOCAL_COLUMNS {
                let local = if same_source {
                    existing.and_then(|old| old.get(*column)).cloned()
                } else {
                    None
                };
                row.insert(
                    (*column).into(),
                    local.unwrap_or_else(|| repository_local_default(column)),
                );
            }
        }
        if table == "rules" {
            // 只恢复规则元数据；启用关系和同步基线沿用本机，新增规则默认不启用。
            let rule_type = row
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let enabled = existing
                .and_then(|item| item.get("enabled_clis"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let mut enabled_clis = Vec::new();
            for cli in ["claude", "codex"] {
                let keep = (rule_type == "common" || rule_type == cli)
                    && enabled.split(',').any(|item| item == cli);
                if keep {
                    enabled_clis.push(cli);
                }
                for (suffix, default) in [("synced_hash", json!("")), ("synced_at", json!(0))] {
                    let column = format!("{cli}_{suffix}");
                    let value = if keep {
                        existing
                            .and_then(|item| item.get(&column))
                            .cloned()
                            .unwrap_or(default)
                    } else {
                        default
                    };
                    row.insert(column, value);
                }
            }
            row.insert("enabled_clis".into(), json!(enabled_clis.join(",")));
        }
        let values = columns
            .iter()
            .map(|column| match &row[column] {
                Value::Null => SqlValue::Null,
                Value::Number(value) => value
                    .as_i64()
                    .map(SqlValue::Integer)
                    .unwrap_or_else(|| SqlValue::Real(value.as_f64().unwrap_or_default())),
                Value::String(value) => SqlValue::Text(value.clone()),
                value => SqlValue::Text(value.to_string()),
            })
            .collect::<Vec<_>>();
        transaction.execute(&sql, params_from_iter(values))?;
        current.insert(target_key.clone(), row);
        transaction.execute(
            "INSERT INTO restore_applied(table_name, row_key) VALUES (?1, ?2)",
            params![table, target_key],
        )?;
    }
    Ok(())
}
