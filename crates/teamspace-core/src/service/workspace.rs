//! 工作空间目录：任意已存在的本地文件夹，不要求是 Git 仓库。

use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result};

/// 把用户输入的目录规范化为绝对路径并确认它是已存在的目录。
///
/// 只做词法规范化（补全为绝对路径、去掉 `.`、按字面消解 `..`、去掉末尾斜杠），不解析符号链接，
/// 保证存下的路径就是用户所选的目录本身。目录不存在或不是目录时为 invalid。
pub(crate) fn resolve(path: &str) -> Result<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(Error::invalid("请选择工作空间目录"));
    }
    let absolute = std::path::absolute(Path::new(trimmed))
        .map_err(|err| Error::invalid(format!("无法解析工作空间路径「{trimmed}」：{err}")))?;
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    if !normalized.exists() {
        return Err(Error::invalid(format!(
            "工作空间目录不存在：{}",
            normalized.display()
        )));
    }
    if !normalized.is_dir() {
        return Err(Error::invalid(format!(
            "工作空间目录不存在：{} 不是目录",
            normalized.display()
        )));
    }
    Ok(normalized)
}

/// 路径的字符串形式（存库与返回给前端时使用）。
pub(crate) fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorKind;

    #[test]
    fn strips_trailing_slash_and_dot_segments() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_string_lossy().into_owned();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        let expected = dir.path().join("a");
        assert_eq!(resolve(&format!("{base}/a/")).unwrap(), expected);
        assert_eq!(resolve(&format!("  {base}/./a/b/.. ")).unwrap(), expected);
        assert!(!display(&resolve(&format!("{base}/a//")).unwrap()).ends_with('/'));
    }

    #[test]
    fn relative_paths_become_absolute() {
        let resolved = resolve(".").unwrap();
        assert!(resolved.is_absolute());
        assert_eq!(resolved, std::env::current_dir().unwrap());
    }

    #[test]
    fn rejects_missing_blank_and_file_paths() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        std::fs::write(&file, "x").unwrap();
        for path in [
            dir.path().join("missing").to_string_lossy().into_owned(),
            file.to_string_lossy().into_owned(),
        ] {
            let err = resolve(&path).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Invalid);
            assert!(err.to_string().contains("工作空间目录不存在"), "{err}");
        }
        assert_eq!(resolve("  ").unwrap_err().kind(), ErrorKind::Invalid);
    }
}
