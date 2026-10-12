//! 对外错误类型。
//!
//! 所有命令失败时返回 [`Error`]，序列化为 `{ kind, message }`（TS 类型 `ErrorPayload`）。
//! `message` 是可以直接展示给用户的中文。
//!
//! 种类约定：
//! - `invalid`：输入本身不合法（必填为空、格式错误、路径越界等），改输入即可。
//! - `not_found`：引用的对象不存在、已归档或不属于该团队。
//! - `conflict`：输入合法但与当前状态冲突（重名、非法状态流转、依赖成环、
//!   就绪条件不满足、岗位仍有未结束任务等）。
//! - `internal`：存储或系统故障，不是用户能直接修正的问题。

use serde::{Deserialize, Serialize, Serializer};
use ts_rs::TS;

/// 错误种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorKind {
    Invalid,
    NotFound,
    Conflict,
    Internal,
}

/// 命令失败时前端收到的错误体。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ErrorPayload {
    pub kind: ErrorKind,
    /// 给用户看的中文说明。
    pub message: String,
}

/// 内核错误。每个变体携带给用户看的中文说明。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Internal(String),
}

impl Error {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Invalid(_) => ErrorKind::Invalid,
            Self::NotFound(_) => ErrorKind::NotFound,
            Self::Conflict(_) => ErrorKind::Conflict,
            Self::Internal(_) => ErrorKind::Internal,
        }
    }

    pub fn payload(&self) -> ErrorPayload {
        ErrorPayload {
            kind: self.kind(),
            message: self.to_string(),
        }
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.payload().serialize(serializer)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Self::Internal(format!("数据存储出错：{err}"))
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Internal(format!("文件系统出错：{err}"))
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::Internal(format!("数据格式出错：{err}"))
    }
}

/// 内核统一的结果类型。
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_kind_and_message() {
        let value = serde_json::to_value(Error::not_found("团队不存在")).unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "kind": "not_found", "message": "团队不存在" })
        );
    }
}
