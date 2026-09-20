pub mod crypto;
pub mod merge;
pub mod package;
pub mod service;
pub mod webdav;

use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum SyncError {
    Cancelled,
    Conflict,
    Crypto(String),
    Format(String),
    Io(std::io::Error),
    Database(rusqlite::Error),
    Network(String),
}

impl Display for SyncError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "同步已取消"),
            Self::Conflict => write!(f, "远端内容已变化，需要重新合并"),
            Self::Crypto(v) | Self::Format(v) | Self::Network(v) => f.write_str(v),
            Self::Io(v) => write!(f, "文件操作失败: {v}"),
            Self::Database(v) => write!(f, "同步数据库失败: {v}"),
        }
    }
}

impl std::error::Error for SyncError {}
impl From<std::io::Error> for SyncError {
    fn from(v: std::io::Error) -> Self {
        Self::Io(v)
    }
}
impl From<rusqlite::Error> for SyncError {
    fn from(v: rusqlite::Error) -> Self {
        Self::Database(v)
    }
}

pub type Result<T> = std::result::Result<T, SyncError>;
