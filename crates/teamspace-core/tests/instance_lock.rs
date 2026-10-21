//! 实例独占权：一个数据库同一时间只允许一个实例（D26 决策 1）。
//!
//! 第二个实例在打开数据库、恢复执行之前就要拿到独占权：取不到就直接报错退出，
//! 一个执行都不碰——否则第一个实例正在跑的执行会被当成「上次退出残留的」标记为已中断，
//! 并在 Agent 还在写文件时被提交。

mod common;

use common::*;
use teamspace_core::model::RunStatus;
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{ErrorKind, HostConfig, Teamspace};

/// 第一个实例的执行还在跑时，第二个实例打不开同一个数据库，第一个执行的状态不变。
#[test]
fn a_second_instance_cannot_open_a_database_that_is_in_use() {
    let e = env(false);
    let task = e.task("[mock:hang]");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);

    // 第二个实例：取不到实例独占权就直接报错退出（不碰任何执行）。
    let db = e.dir.path().join("data/t.db");
    let err = match Teamspace::open_with(&db, Box::new(LocalProbe::new()), HostConfig::default()) {
        Ok(_) => panic!("第二个实例不应打开同一个数据库"),
        Err(err) => err,
    };
    assert_eq!(err.kind(), ErrorKind::Conflict, "{err}");
    assert!(err.to_string().contains("另一个 Teamspace 实例"), "{err}");

    // 第一个实例的执行没有被第二个实例动过：仍然是执行中，不是已中断。
    let run = e.ts.get_run(&run.id).unwrap();
    assert_eq!(run.status, RunStatus::Running, "{run:#?}");
    assert!(run.failure.is_none(), "{run:#?}");
}

/// 第一个实例退出（释放独占权）后，第二个实例可以打开同一个数据库，并接管上次残留的执行。
#[test]
fn the_database_can_be_opened_after_the_first_instance_exits() {
    let e = env(false);
    let task = e.task("[mock:hang]");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    let db = e.dir.path().join("data/t.db");

    // 第一个实例退出：终止执行中的 Agent、释放独占权。
    e.ts.shutdown();
    e.ts.release_instance();

    // 第二个实例这时可以打开同一个数据库。
    let second = Teamspace::open_with(&db, Box::new(LocalProbe::new()), HostConfig::default())
        .expect("第一个实例退出后应能打开同一个数据库");
    // 上次残留的执行被标记为已中断。
    let run = second.get_run(&run.id).unwrap();
    assert_eq!(run.status, RunStatus::Interrupted, "{run:#?}");
    assert!(run.failure.as_deref().unwrap().contains("中断"), "{run:#?}");
}
