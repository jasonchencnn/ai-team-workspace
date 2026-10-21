//! 工具失败时给 Leader 看的中文说明。
//!
//! 两类失败：与桌面应用通信失败（应用没运行），以及 Leader 给的名字/编号解析不到目标。
//! 后端拒绝时（权限边界、状态冲突等）不经过这里，原样带回内核的中文错误。

use crate::client::ControlError;

/// 工具错误。
#[derive(Debug, Clone)]
pub enum ToolError {
    /// 与 Teamspace 应用的通信失败。
    Control(ControlError),
    /// 提示信息（中文），如「找不到岗位」并附上候选。
    Message(String),
}

impl ToolError {
    /// 给 Leader 看的中文说明。
    pub fn message(&self) -> String {
        match self {
            Self::Control(err) => err.message(),
            Self::Message(message) => message.clone(),
        }
    }
}

impl From<ControlError> for ToolError {
    fn from(err: ControlError) -> Self {
        Self::Control(err)
    }
}
