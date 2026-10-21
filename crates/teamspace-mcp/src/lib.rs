//! Teamspace 的 MCP stdio 服务（D26）。
//!
//! 外部会话（Claude Code / Codex 等）通过它把请求转给桌面应用开的本机控制 socket
//! （`teamspace-control.sock`），权限边界在桌面应用的内核里把守：
//! MCP 只做转发，不打开数据库、不启动 Agent（D26）。
//!
//! 模块分工：
//! - [`client`]：控制 socket 客户端（一行一个 JSON 请求，收到 `ok` 或 `err`）。
//! - [`error`]：工具失败时给 Leader 看的中文说明。
//! - [`resolve`]：把 Leader 说的岗位名、任务编号解析成 id（先调用查看命令再解析）。
//! - [`params`]：各工具的参数结构（字段说明即 JSON Schema 里的 description）。
//! - [`tools`]：MCP 工具本身。
#![forbid(unsafe_code)]

pub mod client;
pub mod error;
pub mod params;
pub mod resolve;
pub mod tools;
