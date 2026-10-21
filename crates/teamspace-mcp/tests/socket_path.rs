//! 应用数据目录与控制 socket 路径的算法在两个 crate 里各写了一份
//! （`teamspace-mcp` 不能依赖 `teamspace-core`，只能各写一份）。
//!
//! 两边一旦漂移，生成的配置会指向 MCP 永远连不上的 socket，而且没有测试能发现。
//! 这里断言两边算出来的一致。

#[test]
fn both_crates_compute_the_same_default_socket_path() {
    assert_eq!(
        teamspace_core::leader::default_socket_path(),
        teamspace_mcp::client::default_socket_path(),
        "两个 crate 算出的默认 socket 路径必须一致"
    );
    assert_eq!(
        teamspace_core::leader::app_data_dir(),
        teamspace_mcp::client::app_data_dir(),
        "两个 crate 算出的应用数据目录必须一致"
    );
}

#[test]
fn both_crates_use_the_same_socket_names() {
    assert_eq!(
        teamspace_core::leader::SOCKET_FILE_NAME,
        teamspace_mcp::client::SOCKET_NAME,
    );
    assert_eq!(
        teamspace_core::leader::SOCKET_ENV,
        teamspace_mcp::client::SOCKET_ENV,
    );
    assert_eq!(
        teamspace_core::leader::APP_IDENTIFIER,
        teamspace_mcp::client::APP_IDENTIFIER,
    );
}
