//! 独立的 Mock ACP Agent 可执行文件，供内核的集成测试以真实子进程方式握手。
//! 正式应用中由宿主程序以 `--teamspace-mock-agent` 参数运行同一份实现。
#![forbid(unsafe_code)]

fn main() {
    if let Err(err) = teamspace_core::mock_agent::run_stdio() {
        eprintln!("teamspace-mock-agent: {err}");
        std::process::exit(1);
    }
}
