//! Human 允许的写入授权（Q-01 与交叉审查 06a #1），纯数据结构，不访问文件系统和数据库。
//!
//! - 只有 Human 允许的编辑（`edit`）请求才产生授权，路径为这次编辑的目标（规范化后精确匹配）；
//! - `allow_once`：只放行一次对应的 `fs/write_text_file`，用掉即失效；对应的工具调用结束
//!   （completed / failed）时未用掉的也失效；
//! - `allow_always`：本次执行内对同一路径持续有效；
//! - 工作日志的非追加改写：不论哪种允许，都只放行一次（D14/D17：改动已有内容每次都需确认）。

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Grant {
    tool_call_id: String,
    path: PathBuf,
    always: bool,
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
    /// 记录 Human 对工具调用 `tool_call_id` 的允许；`always` 为「本次执行内总是允许」。
    pub(crate) fn add(&mut self, tool_call_id: &str, paths: Vec<PathBuf>, always: bool) {
        for path in paths {
            self.0.push(Grant {
                tool_call_id: tool_call_id.to_owned(),
                path,
                always,
            });
        }
    }

    /// 是否有对 `path` 的授权（不消费）。
    pub(crate) fn has(&self, path: &Path) -> bool {
        self.0.iter().any(|g| g.path == path)
    }

    /// 普通文件写入：优先使用持续有效的授权；否则消费一条一次性授权。
    pub(crate) fn take(&mut self, path: &Path) -> Option<GrantUse> {
        if self.0.iter().any(|g| g.always && g.path == path) {
            return Some(GrantUse::Always);
        }
        let index = self.0.iter().position(|g| g.path == path)?;
        self.0.remove(index);
        Some(GrantUse::Once)
    }

    /// 工作日志的非追加改写：消费一条授权（持续有效的也只用一次）。
    pub(crate) fn take_once(&mut self, path: &Path) -> bool {
        let index = self
            .0
            .iter()
            .position(|g| !g.always && g.path == path)
            .or_else(|| self.0.iter().position(|g| g.path == path));
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
