//! 共享文件页（`4:435`）的文件系统部分：读取工作空间里的 Markdown。
//!
//! 只做文件系统操作、不访问数据库，由 [`crate::api`] 在数据库锁之外调用。读取边界一次校验清楚：
//! 路径必须是工作空间内的相对路径、不含 `..`、是 `.md`，**解析符号链接后仍在工作空间内**；
//! 文件不存在或工作空间目录已移动都返回中文错误，超大文件截断而不是整份读进内存。
//!
//! 列出文件（[`crate::probe::SystemProbe::list_markdown_files`]）由探测实现负责，因为它要和搜索
//! 共用同一套「哪些目录值得看」的规则；本模块只管读。

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::model::MarkdownFile;

/// 读取 Markdown 的内容上限（256 KiB）；超过时截断并在 `truncated` 里说明。
///
/// 与验收 diff 的单文件补丁上限 [`crate::run::git_review::MAX_PATCH_BYTES`] 取同一个值：共享文件页
/// 把内容整段交给前端渲染，上限就是「渲染一次要多久」的预算。1 MiB 的文档在浏览器主线程上一次
/// react-markdown 渲染要 20 秒以上，页面会整段卡住（连文件列表都刷不出来）；256 KiB 约 1 秒，
/// 页面始终可交互。完整内容在工作空间里，用编辑器打开即可。
pub const MAX_MARKDOWN_BYTES: u64 = 256 * 1024;

/// 校验结果：文件在哪里、以什么写法回显。
#[derive(Debug)]
pub(crate) struct Located {
    /// 文件的真实路径（已解析符号链接），必然位于工作空间之内。
    pub real: PathBuf,
    /// 请求里的相对路径（已去掉锚点、`.` 与重复分隔符），**不解析符号链接**，
    /// 因此与文件列表里那一项写法一致，页面选中态与回显都对得上。
    pub display: String,
}

/// 校验用户给的路径，解析成工作空间内的真实文件。
///
/// 顺序即错误优先级：空 → 绝对路径 → `..` → 扩展名 → 工作空间目录 → 文件是否存在 → 解析符号链接后
/// 是否还在工作空间内。先做便宜的词法检查，避免为一个明显越界的输入去碰文件系统。
pub(crate) fn resolve(workspace: &Path, raw: &str) -> Result<Located> {
    // 引用可以带锚点（`plans/task.md#验收`），路径部分才是文件。
    let without_anchor = raw.split_once('#').map_or(raw, |(path, _)| path);
    let raw = without_anchor.trim().replace('\\', "/");
    if raw.is_empty() {
        return Err(Error::invalid("请选择要查看的文件"));
    }
    let bytes = raw.as_bytes();
    let is_drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if raw.starts_with('/') || is_drive {
        return Err(Error::invalid(format!(
            "文件路径必须是工作空间内的相对路径：{raw}"
        )));
    }

    let mut relative = PathBuf::new();
    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                return Err(Error::invalid(format!("文件路径不能包含「..」：{raw}")));
            }
            other => relative.push(other),
        }
    }
    if relative.as_os_str().is_empty() {
        return Err(Error::invalid("请选择要查看的文件"));
    }
    if !relative
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        return Err(Error::invalid(format!(
            "共享文件只显示 Markdown（.md）文件：{raw}"
        )));
    }

    let root = workspace.canonicalize().map_err(|_| {
        Error::invalid(format!(
            "工作空间目录不存在或已移动：{}",
            workspace.display()
        ))
    })?;
    let real = root
        .join(&relative)
        .canonicalize()
        .map_err(|_| Error::not_found(format!("文件不存在或已被移动：{raw}")))?;
    if !real.starts_with(&root) {
        return Err(Error::invalid(format!(
            "文件在工作空间之外，不予显示：{raw}"
        )));
    }
    if !real.is_file() {
        return Err(Error::invalid(format!("只能查看文件，不能查看目录：{raw}")));
    }
    Ok(Located {
        display: relative_display(&root, &root.join(&relative)),
        real,
    })
}

/// 读取工作空间内的一个 Markdown 文件，返回内容、大小、最后修改时间。
///
/// 读之前先 [`resolve`]（校验全部在 [`resolve`] 里），读的时候最多取 [`MAX_MARKDOWN_BYTES`] 字节，
/// 超出的部分不读进内存；截断落在 UTF-8 字符边界上，预览里不会出现半个字或替换字符。
pub(crate) fn read(workspace: &Path, team_id: &str, raw: &str) -> Result<MarkdownFile> {
    let Located { real, display } = resolve(workspace, raw)?;
    let meta = fs::metadata(&real)
        .map_err(|_| Error::not_found(format!("文件不存在或已被移动：{raw}")))?;
    let size = meta.len();
    let limit = size.min(MAX_MARKDOWN_BYTES);
    let mut buffer = Vec::with_capacity(limit as usize);
    fs::File::open(&real)
        .and_then(|file| file.take(limit).read_to_end(&mut buffer))
        .map_err(|err| Error::internal(format!("无法读取文件：{err}")))?;

    // 文件本身不是合法 UTF-8 时用替换字符兜住（能读就显示），随后把内容裁到不超过上限的最后一个
    // 字符边界：上限可能正好切在多字节字符中间，裁完的预览仍是完整的一串字符。
    let mut content = String::from_utf8_lossy(&buffer).into_owned();
    let mut end = content.len().min(MAX_MARKDOWN_BYTES as usize);
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }
    content.truncate(end);

    let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    Ok(MarkdownFile {
        team_id: team_id.to_owned(),
        path: display,
        content,
        truncated: size > MAX_MARKDOWN_BYTES,
        size,
        updated_at: chrono::DateTime::<chrono::Utc>::from(modified)
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string(),
    })
}

/// 真实路径相对工作空间真实根的形式（`/` 分隔），用于回显和后续跳转。
fn relative_display(root: &Path, real: &Path) -> String {
    real.strip_prefix(root)
        .unwrap_or(real)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("plans")).unwrap();
        fs::write(dir.path().join("plans/task.md"), "# 任务\n").unwrap();
        dir
    }

    #[test]
    fn resolves_a_plain_relative_markdown_path() {
        let dir = workspace();
        let found = resolve(dir.path(), "plans/task.md").unwrap();
        assert!(found.real.is_file());
        assert!(found.real.starts_with(dir.path().canonicalize().unwrap()));
        assert_eq!(found.display, "plans/task.md");
        // 锚点、`\` 分隔符与 `./` 都能吃下，回显仍是 `/` 分隔的相对路径。
        assert_eq!(
            resolve(dir.path(), "./plans\\task.md#验收")
                .unwrap()
                .display,
            "plans/task.md"
        );
    }

    #[test]
    fn rejects_absolute_and_parent_paths() {
        let dir = workspace();
        let absolute = dir.path().join("plans/task.md").display().to_string();
        for raw in [
            absolute.as_str(),
            "../outside.md",
            "plans/../../etc/passwd.md",
        ] {
            let err = resolve(dir.path(), raw).unwrap_err();
            assert_eq!(err.kind(), crate::ErrorKind::Invalid, "{raw}");
        }
        for raw in ["", "   ", "./"] {
            let err = resolve(dir.path(), raw).unwrap_err();
            assert_eq!(err.kind(), crate::ErrorKind::Invalid, "{raw:?}");
        }
    }

    #[test]
    fn rejects_non_markdown_before_touching_the_disk() {
        let dir = workspace();
        // 文件不存在也要先说「不是 Markdown」：类型不对和文件丢失是两件事。
        let err = resolve(dir.path(), "素材清单.txt").unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Invalid);
        assert!(err.to_string().contains(".md"), "{err}");
        // 目录同样不是可读的文件。
        let err = resolve(dir.path(), "plans").unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Invalid);
    }

    #[test]
    fn missing_file_is_not_found() {
        let dir = workspace();
        let err = resolve(dir.path(), "plans/missing.md").unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_out_of_the_workspace_is_rejected() {
        let dir = workspace();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.md"), "不该被读到\n").unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.md"), dir.path().join("leak.md"))
            .unwrap();
        let err = resolve(dir.path(), "leak.md").unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Invalid);
        assert!(err.to_string().contains("工作空间之外"), "{err}");
    }

    #[test]
    fn reads_content_size_and_time() {
        let dir = workspace();
        let file = read(dir.path(), "team", "plans/task.md").unwrap();
        assert_eq!(file.content, "# 任务\n");
        assert_eq!(file.path, "plans/task.md");
        assert!(!file.truncated);
        assert_eq!(file.size, 9); // 字节数：「# 任务\n」里的中文各占 3 字节
        assert!(file.updated_at.ends_with('Z'), "{}", file.updated_at);
    }

    #[test]
    fn truncates_oversized_files_instead_of_failing() {
        let dir = workspace();
        let big = "x".repeat(MAX_MARKDOWN_BYTES as usize + 4096);
        fs::write(dir.path().join("big.md"), &big).unwrap();
        let file = read(dir.path(), "team", "big.md").unwrap();
        assert!(file.truncated);
        assert_eq!(file.size, big.len() as u64);
        assert_eq!(file.content.len(), MAX_MARKDOWN_BYTES as usize);
    }

    #[test]
    fn a_file_at_the_limit_is_not_marked_truncated() {
        let dir = workspace();
        let exact = "x".repeat(MAX_MARKDOWN_BYTES as usize);
        fs::write(dir.path().join("exact.md"), &exact).unwrap();
        let file = read(dir.path(), "team", "exact.md").unwrap();
        assert!(!file.truncated, "正好等于上限不算截断");
        assert_eq!(file.content.len(), MAX_MARKDOWN_BYTES as usize);
    }

    #[test]
    fn truncation_lands_on_a_utf8_character_boundary() {
        let dir = workspace();
        // 「规」占 3 字节；把前缀凑到上限前 2 字节，读窗口就会正好切在这个字的中间。
        let cap = MAX_MARKDOWN_BYTES as usize;
        let head = "# ".repeat(cap / 2 - 1);
        assert_eq!(head.len(), cap - 2, "上限落在「规」的第三个字节上");
        fs::write(dir.path().join("cn.md"), format!("{head}规划说明")).unwrap();

        let file = read(dir.path(), "team", "cn.md").unwrap();
        assert!(file.truncated);
        assert_eq!(
            file.size,
            (cap + 10) as u64,
            "「规划说明」4 个汉字各 3 字节"
        );
        // 半个字符被裁掉，预览里既没有半个字也没有替换字符。
        assert_eq!(file.content, head);
        assert!(!file.content.contains('\u{fffd}'));
    }

    #[test]
    fn missing_workspace_reports_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("moved");
        let err = resolve(&gone, "plans/task.md").unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Invalid);
        assert!(err.to_string().contains("工作空间目录不存在"), "{err}");
    }
}
