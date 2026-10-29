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
fn both_crates_pick_the_same_socket_for_short_and_long_data_dirs() {
    let short = std::path::Path::new("/tmp/teamspace-data");
    let long = std::path::PathBuf::from("/").join("d".repeat(120));
    for dir in [short, long.as_path()] {
        assert_eq!(
            teamspace_core::leader::socket_path_in(dir),
            teamspace_mcp::client::socket_path_in(dir),
            "数据目录 {} 对应的 socket 两边必须一致",
            dir.display()
        );
    }
    assert_eq!(
        teamspace_core::leader::MAX_SOCKET_PATH_BYTES,
        teamspace_mcp::client::MAX_SOCKET_PATH_BYTES,
    );
}

#[test]
fn the_default_socket_path_fits_in_sun_path() {
    let path = teamspace_mcp::client::default_socket_path();
    assert!(
        path.as_os_str().len() <= teamspace_mcp::client::MAX_SOCKET_PATH_BYTES,
        "默认 socket 路径超过 sun_path 上限：{}",
        path.display()
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
    assert_eq!(
        teamspace_core::leader::SHORT_SOCKET_HOME_DIR,
        teamspace_mcp::client::SHORT_SOCKET_HOME_DIR,
    );
}
