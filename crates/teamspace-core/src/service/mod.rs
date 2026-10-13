//! 业务服务：每个函数在调用方提供的连接（通常是事务）上执行，校验规则并写活动记录。

pub(crate) mod event;
pub(crate) mod folder;
pub(crate) mod harness;
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
}
