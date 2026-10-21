//! Teamspace 的 MCP stdio 服务（可执行入口）。
//!
//! stdout 留给 MCP 协议，stderr 是普通日志。启动后一直服务到标准输入关闭。
//!
//! 控制 socket 的路径：默认是 macOS 应用数据目录下的
//! `~/Library/Application Support/com.ai-team-workspace.desktop/teamspace-control.sock`，
//! 可用环境变量 `TEAMSPACE_CONTROL_SOCKET` 覆盖（测试用）。

use rmcp::ServiceExt;
use rmcp::transport::stdio;
use teamspace_mcp::client::ControlClient;
use teamspace_mcp::tools::TeamspaceServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = ControlClient::from_env();
    eprintln!(
        "teamspace-mcp {} 已启动，控制 socket：{}",
        env!("CARGO_PKG_VERSION"),
        client.path().display()
    );
    let service = TeamspaceServer::new(client).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
