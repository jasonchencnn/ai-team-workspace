//! Teamspace 的 MCP stdio 服务（D26：MCP 只做转发，真正的执行由桌面应用负责）。
//!
//! 外部会话（Claude Code / Codex 等）通过它把请求转给桌面应用开的本机控制 socket
//! （`teamspace-control.sock`），权限边界在桌面应用的内核里把守。
//!
//! 当前只打印版本：M4-02 在此基础上实现 MCP 工具与 stdio 传输（届时 stdout 留给 MCP 协议）。

fn main() {
    println!("teamspace-mcp {}", env!("CARGO_PKG_VERSION"));
}
