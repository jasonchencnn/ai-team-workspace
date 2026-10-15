//! Human 允许的写入与读取授权（Q-01、交叉审查 06a #1、D18），纯数据结构，不访问文件系统和数据库。
//!
//! - 写入授权：只有 Human 允许的编辑（`edit`）请求才产生，路径为这次编辑的目标（规范化后精确匹配）；
//! - 读取授权（D18）：Human 允许的读取、搜索请求产生，路径为请求涉及的路径，供之后的
//!   `fs/read_text_file` 使用；读取授权与写入授权互不放行；
//! - `allow_once`：只放行一次对应的 `fs/write_text_file`，用掉即失效；对应的工具调用结束
//!   （completed / failed）时未用掉的也失效；
//! - `allow_always`：本次执行内对同一路径持续有效；
//! - 工作日志的非追加改写：不论哪种允许，都只放行一次（D14/D17：改动已有内容每次都需确认）。

use std::path::{Path, PathBuf};

/// 授权的种类：写入与读取互不放行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    Write,
    Read,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Grant {
    tool_call_id: String,
    path: PathBuf,
    always: bool,
    access: Access,
}

/// 一次执行内的写入授权。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Grants(Vec<Grant>);

/// 使用了哪种授权。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrantUse {
    Once,
    Always,
}

impl GrantUse {
    /// 写入记录里的原因。
    pub(crate) fn reason(self) -> &'static str {
        match self {
            GrantUse::Once => "Human 已允许对应的权限请求（仅这一次）",
            GrantUse::Always => "Human 已允许对应的权限请求（本次执行内总是允许）",
        }
    }
}

impl Grants {
    fn push(&mut self, access: Access, tool_call_id: &str, paths: Vec<PathBuf>, always: bool) {
        for path in paths {
            self.0.push(Grant {
                tool_call_id: tool_call_id.to_owned(),
                path,
                always,
                access,
            });
        }
    }

    /// 记录 Human 对编辑请求 `tool_call_id` 的允许（写入授权）；`always` 为「本次执行内总是允许」。
    pub(crate) fn add(&mut self, tool_call_id: &str, paths: Vec<PathBuf>, always: bool) {
        self.push(Access::Write, tool_call_id, paths, always);
    }

    /// 记录 Human 对读取、搜索请求 `tool_call_id` 的允许（读取授权，D18）。
    pub(crate) fn add_read(&mut self, tool_call_id: &str, paths: Vec<PathBuf>, always: bool) {
        self.push(Access::Read, tool_call_id, paths, always);
    }

    /// 是否有对 `path` 的写入授权（不消费）。
    pub(crate) fn has(&self, path: &Path) -> bool {
        self.0
            .iter()
            .any(|g| g.access == Access::Write && g.path == path)
    }

    fn take_access(&mut self, access: Access, path: &Path) -> Option<GrantUse> {
        let matches = |g: &Grant| g.access == access && g.path == path;
        if self.0.iter().any(|g| g.always && matches(g)) {
            return Some(GrantUse::Always);
        }
        let index = self.0.iter().position(matches)?;
        self.0.remove(index);
        Some(GrantUse::Once)
    }

    /// 普通文件写入：优先使用持续有效的写入授权；否则消费一条一次性的。
    pub(crate) fn take(&mut self, path: &Path) -> Option<GrantUse> {
        self.take_access(Access::Write, path)
    }

    /// 读取工作空间之外的文件（D18）：优先使用持续有效的读取授权；否则消费一条一次性的。
    pub(crate) fn take_read(&mut self, path: &Path) -> Option<GrantUse> {
        self.take_access(Access::Read, path)
    }

    /// 工作日志的非追加改写：消费一条写入授权（持续有效的也只用一次）。
    pub(crate) fn take_once(&mut self, path: &Path) -> bool {
        let write = |g: &Grant| g.access == Access::Write && g.path == path;
        let index = self
            .0
            .iter()
            .position(|g| !g.always && write(g))
            .or_else(|| self.0.iter().position(write));
        match index {
            Some(index) => {
                self.0.remove(index);
                true
            }
            None => false,
        }
    }

    /// 工具调用结束：它未用掉的一次性授权失效。
    pub(crate) fn expire_call(&mut self, tool_call_id: &str) {
        self.0
            .retain(|g| g.always || g.tool_call_id != tool_call_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_and_write_grants_do_not_cross() {
        let mut g = Grants::default();
        g.add_read("r1", vec![p("/x")], false);
        assert!(!g.has(&p("/x")), "读取授权不是写入授权");
        assert_eq!(g.take(&p("/x")), None);
        assert!(!g.take_once(&p("/x")));
        assert_eq!(g.take_read(&p("/x")), Some(GrantUse::Once));
        assert_eq!(g.take_read(&p("/x")), None, "允许一次只读一次");

        g.add("w1", vec![p("/y")], true);
        assert_eq!(g.take_read(&p("/y")), None, "写入授权不是读取授权");
        assert_eq!(g.take(&p("/y")), Some(GrantUse::Always));

        g.add_read("r2", vec![p("/z")], true);
        assert_eq!(g.take_read(&p("/z")), Some(GrantUse::Always));
        assert_eq!(g.take_read(&p("/z")), Some(GrantUse::Always));
        g.add_read("r3", vec![p("/w")], false);
        g.expire_call("r3");
        assert_eq!(g.take_read(&p("/w")), None);
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn once_is_used_up_and_always_persists() {
        let mut g = Grants::default();
        g.add("c1", vec![p("/a")], false);
        g.add("c2", vec![p("/b")], true);
        assert_eq!(g.take(&p("/a")), Some(GrantUse::Once));
        assert_eq!(g.take(&p("/a")), None, "一次性授权用掉即失效");
        assert_eq!(g.take(&p("/b")), Some(GrantUse::Always));
        assert_eq!(g.take(&p("/b")), Some(GrantUse::Always));
        assert_eq!(g.take(&p("/c")), None);
    }

    #[test]
    fn finished_tool_calls_expire_their_once_grants() {
        let mut g = Grants::default();
        g.add("c1", vec![p("/a"), p("/b")], false);
        g.add("c2", vec![p("/a")], true);
        g.add("c3", vec![p("/c")], false);
        g.expire_call("c1");
        assert!(g.has(&p("/a")), "持续有效的授权不受影响");
        assert!(!g.has(&p("/b")));
        assert!(g.has(&p("/c")));
        g.expire_call("c2");
        assert!(g.has(&p("/a")));
    }

    #[test]
    fn journal_overwrites_consume_even_always_grants() {
        let mut g = Grants::default();
        g.add("c1", vec![p("/j")], true);
        assert!(g.take_once(&p("/j")));
        assert!(!g.take_once(&p("/j")), "改写工作日志每次都需确认");
        g.add("c2", vec![p("/j")], false);
        g.add("c3", vec![p("/j")], true);
        // 先用一次性的那条。
        assert!(g.take_once(&p("/j")));
        assert!(g.has(&p("/j")));
        assert!(g.take_once(&p("/j")));
        assert!(!g.has(&p("/j")));
    }
}
