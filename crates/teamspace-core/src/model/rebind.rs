//! 岗位换绑（M6-01）。
//!
//! 换绑只改变岗位与 Harness 实例的绑定，不改变岗位、任务与历史（原产品原则）。三种方式：
//! - `after_current`：当前执行结束后再生效；没有进行中的执行时立即生效。
//! - `cancel_and_rerun`：取消进行中的执行，立即换绑，并在新 Harness 上对该任务重新开始执行。
//! - `manual_takeover`：取消进行中的执行，任务转为受阻（人工接管），立即换绑。
//!
//! 换绑属 Human 的控制权（D8、D32）：Leader 不能发起，也不能撤销。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 换绑方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RebindMode {
    /// 当前执行结束后再生效（默认）。
    AfterCurrent,
    /// 取消当前执行，立即换绑并在新 Harness 上重新执行该任务。
    CancelAndRerun,
    /// 取消当前执行，任务转为受阻（人工接管），立即换绑。
    ManualTakeover,
}

impl RebindMode {
    /// 默认方式：当前执行结束后再生效。
    pub const DEFAULT: Self = Self::AfterCurrent;

    /// 展示用的中文名。
    pub fn label(self) -> &'static str {
        match self {
            Self::AfterCurrent => "当前执行结束后更换",
            Self::CancelAndRerun => "取消并重新执行",
            Self::ManualTakeover => "人工接管",
        }
    }
}

/// 待生效的换绑：换绑已登记，但当前绑定仍是旧实例。
///
/// 待生效期间新的派工仍走旧绑定，直到换绑生效（D8：换绑只能由 Human 执行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PendingBinding {
    pub id: String,
    pub role_id: String,
    /// 岗位名（留痕用：岗位归档后仍能看出这条换绑是给哪个岗位的）。
    pub role_name: String,
    /// 目标 Harness 实例 id。
    pub harness_id: String,
    /// 目标 Harness 实例名。
    pub harness_name: String,
    /// 发起换绑时岗位的当前绑定实例 id；发起时岗位没有绑定过实例为 `null`。
    ///
    /// 留痕用：待生效期间当前绑定仍是旧实例，这个字段记下「换绑之前绑的是哪个」。
    pub previous_harness_id: Option<String>,
    /// 发起换绑时岗位的当前绑定实例名；发起时岗位没有绑定过实例为 `null`。
    pub previous_harness_name: Option<String>,
    /// 换绑方式。
    pub mode: RebindMode,
    /// 发起时间（UTC RFC3339）。
    pub requested_at: String,
    /// 发起时该岗位进行中的执行 id；没有时为 `null`。
    pub active_run_id: Option<String>,
}
