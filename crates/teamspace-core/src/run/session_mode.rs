//! 会话建立后的权限模式处理（M2.1-02，S-1a）：让 Agent 在执行命令、写文件前向 Teamspace 请求权限。
//!
//! 判断逻辑在 [`crate::acp::permission_mode`]；这里把它接到执行启动流程：`session/new` 或 `session/load`
//! 返回后调用 [`Driver::enforce_permission_mode`]，按需发 `session/set_mode` / `session/set_config_option`，
//! 切换失败或无法切换时把提示写进执行记录（一条状态进度）。不改动权限判断本身（见 [`super::policy`]）。

use serde_json::{Value, json};

use super::{Driver, End, Stop};
use crate::acp::connection::RpcFailure;
use crate::acp::permission_mode::{self, Switch};
use crate::model::{AcpPreset, RunUpdateBody};

impl Driver {
    /// 用户自己的启动参数是否已经指定了权限：自定义预设以外，只看参数里有没有权限相关选项。
    fn user_controls_permissions(&self) -> bool {
        // Mock 的参数是宿主内置的，不算用户指定。
        self.run.harness.preset != AcpPreset::Mock
            && permission_mode::args_set_permissions(&self.run.harness.args)
    }

    /// 根据 `session/new` / `session/load` 的响应，把会话切到「向 Client 请求权限」的模式。
    ///
    /// 只有 Agent 取消、被接管或进程退出时才返回错误；切换本身失败只写提示，执行继续。
    pub(super) fn enforce_permission_mode(
        &mut self,
        session_id: &str,
        response: &Value,
    ) -> std::result::Result<(), End> {
        let plan = permission_mode::plan(response, self.user_controls_permissions());
        let mut warnings = plan.warnings;
        for switch in plan.switches {
            let (method, params) = match &switch {
                Switch::Mode { mode_id } => (
                    "session/set_mode",
                    json!({ "sessionId": session_id, "modeId": mode_id }),
                ),
                Switch::Config { config_id, value } => (
                    "session/set_config_option",
                    json!({ "sessionId": session_id, "configId": config_id, "value": value }),
                ),
            };
            let active = std::sync::Arc::clone(&self.active);
            let result =
                self.conn()
                    .request(method, params, self.settings.startup_timeout, &move || {
                        active.stop() != Stop::None
                    });
            match result {
                Ok(_) => {}
                Err(RpcFailure::Aborted) => {
                    return Err(self.aborted().unwrap_or(End::Cancelled(None)));
                }
                Err(RpcFailure::Closed | RpcFailure::Write(_)) => return Err(self.agent_exited()),
                Err(failure) => {
                    warnings.push(permission_mode::switch_failed(&switch, &failure.describe()))
                }
            }
        }
        for text in warnings {
            let status = self.run.status;
            self.insert(
                None,
                &RunUpdateBody::Status {
                    status,
                    detail: Some(text),
                },
            );
        }
        Ok(())
    }
}
