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
            let current = read_rows(&connection, "main", &table, &columns, &key_column)?;
            let backup = read_rows(&connection, "restore_source", &table, &columns, &key_column)?;
            for (row_key, row) in backup {
                let backup = comparable_row(&connection, &table, &row)?;
                let current = current
                    .get(&row_key)
                    .map(|row| comparable_row(&connection, &table, row))
                    .transpose()?;
                if current.as_ref() != Some(&backup) {
                    differences.push(RestoreRowDifference {
                        table: table.clone(),
                        row_key,
                        current,
                        backup,
                    });
                }
            }
        }
        differences.sort_by(|left, right| {
            (&left.table, &left.row_key).cmp(&(&right.table, &right.row_key))
        });
        Ok(differences)
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
        "SELECT {} FROM {schema}.{}",
        columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", "),
        quote_identifier(table)
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
    connection: &Connection,
    table: &str,
    row: &Map<String, Value>,
) -> Result<Value, ManagerError> {
    if let Some(payload) = row.get("payload_json").and_then(Value::as_str) {
        let expression = comparison_column(table, "payload_json").replace("\"payload_json\"", "?1");
        let normalized: String =
            connection.query_row(&format!("SELECT {expression}"), [payload], |row| row.get(0))?;
        return Ok(serde_json::from_str(&normalized)?);
    }
    let mut value = row.clone();
    value.remove("sort_order");
    Ok(Value::Object(value))
}

pub(super) fn merge_table(
    transaction: &rusqlite::Transaction<'_>,
    table: &str,
) -> Result<(), ManagerError> {
    let columns = read_common_columns(transaction, table)?;
    let key_column = primary_key(transaction, table)?;
    let current = read_rows(transaction, "main", table, &columns, &key_column)?;
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
        let existing = current.get(&row_key);
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
        transaction.execute(
            "INSERT INTO restore_applied(table_name, row_key) VALUES (?1, ?2)",
            params![table, row_key],
        )?;
    }
    Ok(())
}
