#![forbid(unsafe_code)]

fn main() {
    // 作为内置 Mock ACP Agent 运行（由 Teamspace 自己启动，用于测试和演示，不消耗额度）。
    if std::env::args().nth(1).as_deref() == Some(teamspace_core::mock_agent::MOCK_AGENT_FLAG) {
        if let Err(error) = teamspace_core::mock_agent::run_stdio() {
            eprintln!("teamspace mock agent stopped unexpectedly: {error}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = desktop::run() {
        eprintln!("desktop host stopped unexpectedly: {error}");
        std::process::exit(1);
    }
}
