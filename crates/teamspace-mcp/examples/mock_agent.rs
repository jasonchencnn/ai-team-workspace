//! 测试用的 Mock Agent 进程。
//!
//! 复用内核内置的 Mock Agent（`teamspace_core::mock_agent`）：以 ACP Agent 的身份在
//! stdin / stdout 上收发 JSON-RPC，不调用任何模型、不消耗额度。
//!
//! 集成测试用它当 Harness，让「开始执行 → 进度 → 结果 → 验收」这条链路真的跑起来。

fn main() {
    if let Err(err) = teamspace_core::mock_agent::run_stdio() {
        eprintln!("Mock Agent 退出：{err}");
        std::process::exit(1);
    }
}
