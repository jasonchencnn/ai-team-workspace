//! 实例独占权：一个数据库同一时间只允许一个 Teamspace 实例使用。
//!
//! 打开数据库之前先在数据库所在目录里取得独占权（锁文件），取不到就直接报错退出，
//! 不打开数据库、不碰任何执行。否则第二个实例会把第一个实例正在跑的执行当成
//! 「上次退出残留的」标记为已中断，并在 Agent 还在写文件时提交它的工作目录（D26 决策 1）。
//!
//! 锁文件里写的是「进程号 + 唯一标记」：正常退出时自动删除；异常退出留下的残留锁，
//! 下次打开时按进程号判断持有者是否还活着——活着就报错退出，不活着才接管。

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use uuid::Uuid;

use crate::error::{Error, Result};

/// 锁文件的后缀（放在数据库所在目录下，名字跟数据库文件对应：一个目录里可以有多个库）。
const LOCK_SUFFIX: &str = ".lock";

/// 取得独占权的最大尝试次数（清掉残留锁后重试）。
const MAX_ATTEMPTS: usize = 3;

/// 独占权守卫：持有期间别的实例打不开同一个数据库。
#[derive(Debug)]
pub(crate) struct InstanceLock {
    /// 锁文件路径。
    path: PathBuf,
    /// 本次持有的标记（进程号 + 唯一 id）。
    token: String,
    /// 锁文件句柄：持有它让文件不被删掉。
    _file: File,
}

impl InstanceLock {
    /// 取得 `db_path` 所在数据库的独占权；已被别的实例占用时返回错误。
    pub(crate) fn acquire(db_path: &Path) -> Result<Self> {
        let path = lock_path(db_path);
        let token = format!("{}:{}", std::process::id(), Uuid::now_v7());
        for _ in 0..MAX_ATTEMPTS {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    if let Err(err) = write_token(&mut file, &token) {
                        let _ = std::fs::remove_file(&path);
                        return Err(Error::internal(format!(
                            "无法写入实例锁 {}：{err}",
                            path.display()
                        )));
                    }
                    // 确认锁文件里写的是自己的标记：极小概率下别人可能刚好抢到锁。
                    if read_token(&path).as_deref() == Some(token.as_str()) {
                        return Ok(Self {
                            path,
                            token,
                            _file: file,
                        });
                    }
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                    let mut owner = read_token(&path);
                    if owner.is_none() {
                        // 锁文件刚被创建、内容还没写进去：等一下再看，别把别人刚拿到的锁抢了。
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        owner = read_token(&path);
                    }
                    let pid = owner.as_deref().and_then(parse_pid);
                    if let Some(pid) = pid
                        && process_alive(pid)
                    {
                        return Err(Error::conflict(format!(
                            "另一个 Teamspace 实例正在使用这个数据库（进程 {pid}）；\
                             请先关闭那个实例再打开。\
                             若确认没有实例在运行，可删除锁文件 {} 后重试",
                            path.display()
                        )));
                    }
                    // 上次异常退出留下的残留锁：清掉后重试。
                    let _ = std::fs::remove_file(&path);
                }
                Err(err) => {
                    return Err(Error::internal(format!(
                        "无法创建实例锁 {}：{err}",
                        path.display()
                    )));
                }
            }
        }
        Err(Error::conflict(format!(
            "无法取得数据库的独占权（锁文件 {}）：请确认没有其他实例在运行",
            path.display()
        )))
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // 只删自己的锁：内容对不上说明锁已经被别人接管，这时不该删。
        if read_token(&self.path).as_deref() == Some(self.token.as_str()) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// 锁文件路径：数据库所在目录下的 `<数据库文件名>.lock`。
///
/// 一个目录里可以有多个数据库，所以锁文件跟数据库文件一一对应。
fn lock_path(db_path: &Path) -> PathBuf {
    let dir = db_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let name = db_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "teamspace".to_owned());
    dir.join(format!("{name}{LOCK_SUFFIX}"))
}

/// 把标记写进锁文件。
fn write_token(file: &mut File, token: &str) -> std::io::Result<()> {
    file.write_all(token.as_bytes())?;
    file.sync_data()
}

/// 读出锁文件里的标记（文件不在或读不出来时为 `None`）。
fn read_token(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    Some(text.trim().to_owned())
}

/// 从标记里取出进程号。
fn parse_pid(token: &str) -> Option<u32> {
    token.split(':').next()?.parse().ok()
}

/// 进程是否还活着（只判断存在性，不发信号）。
fn process_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // `kill -0 <pid>` 只做存在性与权限探测，不会真的发信号；
        // 探测不了（没有 kill 命令）时按「还活着」处理，不抢别人的锁。
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 取得独占权后，锁文件在且内容是本次的标记。
    #[test]
    fn acquiring_creates_the_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("t.db");
        let lock = InstanceLock::acquire(&db).unwrap();
        let path = dir.path().join("t.db.lock");
        assert!(path.exists());
        assert_eq!(read_token(&path).as_deref(), Some(lock.token.as_str()));
    }

    /// 同一个数据库已经被占用时，第二次取锁失败并给出可操作的中文提示。
    #[test]
    fn a_second_instance_cannot_take_the_same_lock() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("t.db");
        let _lock = InstanceLock::acquire(&db).unwrap();
        let err = InstanceLock::acquire(&db).unwrap_err();
        assert_eq!(err.kind(), crate::error::ErrorKind::Conflict, "{err}");
        assert!(err.to_string().contains("另一个 Teamspace 实例"), "{err}");
    }

    /// 释放后可以再次取得独占权。
    #[test]
    fn the_lock_is_released_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("t.db");
        {
            let _lock = InstanceLock::acquire(&db).unwrap();
            assert!(dir.path().join("t.db.lock").exists());
        }
        assert!(!dir.path().join("t.db.lock").exists());
        let _again = InstanceLock::acquire(&db).unwrap();
    }

    /// 上次异常退出留下的残留锁（进程号已不存在）会被接管。
    #[test]
    fn a_stale_lock_from_a_dead_process_is_taken_over() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("t.db");
        // 找一个几乎肯定不存在的进程号。
        let dead = 4_000_000_000u32;
        std::fs::write(
            dir.path().join("t.db.lock"),
            format!("{dead}:00000000-0000-7000-8000-000000000000"),
        )
        .unwrap();
        let _lock = InstanceLock::acquire(&db).unwrap();
        let token = read_token(&dir.path().join("t.db.lock")).unwrap();
        assert!(
            token.starts_with(&std::process::id().to_string()),
            "{token}"
        );
    }
}
