//! 环境配置档案：Harness 实例可以指定一个 `KEY=VALUE` 格式的文件，执行时把其中的变量加到 Agent
//! 进程的环境里。平台只记文件路径，在启动 Agent 时读取，不保存、不记录、不显示其中的值。
//!
//! 格式：每行一个 `KEY=VALUE`，可带 `export ` 前缀；空行与 `#` 开头的行忽略；值两侧成对的单引号或
//! 双引号会去掉。变量名须为字母、数字、下划线且不以数字开头。

use std::path::Path;

/// 读取并解析环境配置档案。错误说明只含行号，不含内容（可能是密钥）。
pub fn load(path: &Path) -> Result<Vec<(String, String)>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("无法读取环境配置档案 {}：{err}", path.display()))?;
    parse(&text).map_err(|line| {
        format!(
            "环境配置档案 {} 第 {line} 行格式不正确（应为 KEY=VALUE）",
            path.display()
        )
    })
}

/// 解析内容；失败时返回出错的行号（从 1 开始）。
pub fn parse(text: &str) -> Result<Vec<(String, String)>, usize> {
    let mut vars = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").map_or(line, str::trim_start);
        let (key, value) = line.split_once('=').ok_or(index + 1)?;
        let key = key.trim();
        let valid = key
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            return Err(index + 1);
        }
        let value = value.trim();
        let value = ['"', '\'']
            .iter()
            .find_map(|q| value.strip_prefix(*q)?.strip_suffix(*q))
            .unwrap_or(value);
        vars.push((key.to_owned(), value.to_owned()));
    }
    Ok(vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_forms() {
        let vars =
            parse("# 注释\n\nA=1\nexport B = two words \nC=\"quoted value\"\nD='x'\nE=\n_F=a=b\n")
                .unwrap();
        let pairs: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(
            pairs,
            vec![
                ("A", "1"),
                ("B", "two words"),
                ("C", "quoted value"),
                ("D", "x"),
                ("E", ""),
                ("_F", "a=b"),
            ]
        );
    }

    #[test]
    fn rejects_bad_lines_without_echoing_values() {
        assert_eq!(parse("A=1\nnot a pair\n"), Err(2));
        assert_eq!(parse("1A=x"), Err(1));
        assert_eq!(parse("A-B=x"), Err(1));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.env");
        std::fs::write(&path, "SECRET_TOKEN\n").unwrap();
        let err = load(&path).unwrap_err();
        assert!(err.contains("第 1 行"), "{err}");
        assert!(!err.contains("SECRET_TOKEN"), "{err}");
        assert!(load(&dir.path().join("missing")).is_err());
    }
}
