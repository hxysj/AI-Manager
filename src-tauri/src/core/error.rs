use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("未迁移的接口通道：{0}")]
    UnknownChannel(String),
    #[error("路径处理失败：{0}")]
    Path(String),
    #[error("文件处理失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("系统调用失败：{0}")]
    System(String),
    #[error("数据序列化失败：{0}")]
    Json(#[from] serde_json::Error),
    #[error("数据库处理失败：{0}")]
    Sqlite(#[from] rusqlite::Error),
}
