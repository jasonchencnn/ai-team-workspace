//! 业务服务：每个函数在调用方提供的连接（通常是事务）上执行，校验规则并写活动记录。

pub(crate) mod dependency;
pub(crate) mod event;
pub(crate) mod folder;
pub(crate) mod harness;
pub(crate) mod review;
pub(crate) mod role;
pub(crate) mod run;
pub(crate) mod task;
pub(crate) mod team;
pub(crate) mod workspace;

use crate::error::{Error, Result};

/// 名称（团队、岗位、Harness）的字符上限。
pub(crate) const MAX_NAME: usize = 100;
/// 任务标题的字符上限。
pub(crate) const MAX_TITLE: usize = 200;
/// 受阻原因的字符上限。
pub(crate) const MAX_REASON: usize = 500;
/// 描述、职责、工作约定的字符上限。
pub(crate) const MAX_LONG_TEXT: usize = 20_000;
/// 引用文件路径的字符上限。
pub(crate) const MAX_REF_PATH: usize = 1024;
/// 引用文件锚点的字符上限。
pub(crate) const MAX_REF_ANCHOR: usize = 256;

/// 校验必须保持单行的文本：去首尾空白、`required` 时非空、不超过 `max` 个字符，且不含任何
/// 控制字符。用于会被当成文件路径一部分的位置（文件引用路径与锚点）。
pub(crate) fn single_line(label: &str, value: &str, max: usize, required: bool) -> Result<String> {
    let value = value.trim();
    if required && value.is_empty() {
        return Err(Error::invalid(format!("{label}不能为空")));
    }
    if value.chars().count() > max {
        return Err(Error::invalid(format!("{label}不能超过 {max} 个字符")));
    }
    if value.chars().any(char::is_control) {
        return Err(Error::invalid(format!("{label}不能包含换行或控制字符")));
    }
    Ok(value.to_owned())
}

/// 岗位名会被拼进岗位说明的标题行：去首尾空白、`required` 时非空、不超过 `max` 个字符，
/// 且只保留制表符这一个控制字符——它由文件夹名规则转成 `_`，不会断开标题行。
/// NUL 等其余控制字符一律拒绝，避免写坏数据库与文件系统。
pub(crate) fn name_line(label: &str, value: &str, max: usize, required: bool) -> Result<String> {
    let value = value.trim();
    if required && value.is_empty() {
        return Err(Error::invalid(format!("{label}不能为空")));
    }
    if value.chars().count() > max {
        return Err(Error::invalid(format!("{label}不能超过 {max} 个字符")));
    }
    if value.chars().any(|c| c.is_control() && c != '\t') {
        return Err(Error::invalid(format!("{label}不能包含换行或控制字符")));
    }
    Ok(value.to_owned())
}

/// 校验用户输入的文本：去掉首尾空白；`required` 时不能为空；不超过 `max` 个字符；
/// 不含控制字符（换行和制表符除外）。返回去掉首尾空白后的文本。
pub(crate) fn text(label: &str, value: &str, max: usize, required: bool) -> Result<String> {
    let value = value.trim();
    if required && value.is_empty() {
        return Err(Error::invalid(format!("{label}不能为空")));
    }
    if value.chars().count() > max {
        return Err(Error::invalid(format!("{label}不能超过 {max} 个字符")));
    }
    if value
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(Error::invalid(format!("{label}包含不允许的控制字符")));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorKind;

    #[test]
    fn text_checks_required_length_and_control_chars() {
        assert_eq!(text("标题", "  a  ", 3, true).unwrap(), "a");
        assert_eq!(text("描述", "  ", 3, false).unwrap(), "");
        assert_eq!(
            text("描述", "第一行\n\t第二行", 20, false).unwrap(),
            "第一行\n\t第二行"
        );
        assert_eq!(text("标题", "一二三", 3, true).unwrap(), "一二三");
        for (value, needle) in [
            (" ", "不能为空"),
            ("一二三四", "不能超过 3 个字符"),
            ("a\0b", "控制字符"),
            ("a\u{7}b", "控制字符"),
            ("a\rb", "控制字符"),
        ] {
            let err = text("标题", value, 3, true).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Invalid);
            assert!(err.to_string().contains(needle), "{value:?}: {err}");
        }
    }

    #[test]
    fn single_line_and_name_line_reject_control_chars() {
        assert_eq!(single_line("路径", "  a.md  ", 10, true).unwrap(), "a.md");
        assert_eq!(name_line("岗位名称", " 一 ", 4, true).unwrap(), "一");
        // 两者都不接受：NUL、换行、回车，以及超长。
        for bad in ["a\0b", "一\n二", "一\r二", &"x".repeat(11)] {
            assert_eq!(
                single_line("路径", bad, 10, true).unwrap_err().kind(),
                ErrorKind::Invalid,
                "{bad:?}"
            );
            assert_eq!(
                name_line("岗位名称", bad, 10, true).unwrap_err().kind(),
                ErrorKind::Invalid,
                "{bad:?}"
            );
        }
        // 制表符：路径不接受，名称保留（k_fld_002 依赖这一点）。
        assert_eq!(
            single_line("路径", "a\tb", 10, true).unwrap_err().kind(),
            ErrorKind::Invalid
        );
        assert_eq!(
            name_line("岗位名称", "含\t制表符", 10, true).unwrap(),
            "含\t制表符"
        );
        // 必填与长度上限的提示。
        assert!(
            single_line("路径", " ", 3, true)
                .unwrap_err()
                .to_string()
                .contains("不能为空")
        );
        assert!(
            name_line("岗位名称", "一二三四", 3, true)
                .unwrap_err()
                .to_string()
                .contains("不能超过 3 个字符")
        );
    }
}
