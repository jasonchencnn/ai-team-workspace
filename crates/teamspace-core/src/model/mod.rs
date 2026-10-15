//! 对外数据类型（IPC 契约）。
//!
//! 约定：
//! - 字段在 JSON / TS 中为 camelCase，枚举值为 snake_case 字符串。
//! - ID 为 UUID v7 字符串；时间为 UTC RFC3339 字符串，界面按本地时间显示。
//! - 核心对象（团队、岗位、Harness 实例）只归档（`archivedAt`），不硬删除；
//!   任务不归档也不删除，以 `cancelled` 结束。
//! - 输出类型中可空字段为 `T | null`；输入类型中可缺省字段为 `field?: T | null`。
//! - TS 类型由 `pnpm run bindings` 生成到 `apps/desktop/src/bindings/`。

mod event;
mod file;
mod harness;
mod review;
mod role;
mod run;
mod system;
mod task;
mod team;

pub use event::*;
pub use file::*;
pub use harness::*;
pub use review::*;
pub use role::*;
pub use run::*;
pub use system::*;
pub use task::*;
pub use team::*;
