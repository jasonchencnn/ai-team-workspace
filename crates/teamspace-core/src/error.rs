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
    /// 约束冲突映射为 conflict（应用层检查之外的兜底），其余为 internal。
    fn from(err: rusqlite::Error) -> Self {
        use rusqlite::ffi;
        if let rusqlite::Error::SqliteFailure(failure, _) = &err
            && failure.code == rusqlite::ErrorCode::ConstraintViolation
        {
            let message = match failure.extended_code {
                ffi::SQLITE_CONSTRAINT_UNIQUE | ffi::SQLITE_CONSTRAINT_PRIMARYKEY => {
                    "保存失败：已存在相同的记录（名称、标识或前缀重复）"
                }
                ffi::SQLITE_CONSTRAINT_FOREIGNKEY => "保存失败：引用的记录不存在",
                _ => "保存失败：数据不满足约束条件",
            };
            return Self::Conflict(message.to_owned());
        }
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
    fn constraint_violations_map_to_conflict() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE p (id TEXT PRIMARY KEY, name TEXT UNIQUE, n INTEGER CHECK (n > 0));
             CREATE TABLE c (pid TEXT REFERENCES p (id));
             INSERT INTO p VALUES ('a', 'x', 1);",
        )
        .unwrap();
        let cases = [
            ("INSERT INTO p VALUES ('b', 'x', 1)", "已存在相同的记录"),
            ("INSERT INTO p VALUES ('a', 'y', 1)", "已存在相同的记录"),
            ("INSERT INTO c VALUES ('missing')", "引用的记录不存在"),
            ("INSERT INTO p VALUES ('c', 'z', 0)", "不满足约束条件"),
        ];
        for (sql, needle) in cases {
            let err: Error = conn.execute(sql, []).unwrap_err().into();
            assert_eq!(err.kind(), ErrorKind::Conflict, "{sql}");
            assert!(err.to_string().contains(needle), "{sql}: {err}");
        }
        let err: Error = conn.execute("SELECT * FROM nope", []).unwrap_err().into();
        assert_eq!(err.kind(), ErrorKind::Internal);
    }

    #[test]
    fn serializes_as_kind_and_message() {
        let value = serde_json::to_value(Error::not_found("团队不存在")).unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "kind": "not_found", "message": "团队不存在" })
        );
    }
}
