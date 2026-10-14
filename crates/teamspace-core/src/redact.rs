//! 疑似密钥的脱敏（D6：平台不保存密钥，登录由 CLI 自己管理）。
//!
//! Harness 的启动参数和环境变量里常混有 API Key、令牌。Teamspace 需要在多处展示或落盘这些内容
//! （岗位说明、活动、执行快照、失败原因、工作日志），因此写入前都要先经过本模块：
//!
//! - [`args`]：启动参数数组（逐个参数判断，含「名字与值分开」的相邻形式）；
//! - [`text`]：任意文本（Agent 的 stderr、执行失败原因等）。
//!
//! 识别规则（命中才替换为 `***`，避免误伤模型名、开关等正常参数）：
//!
//! 1. 名字里含 key / token / secret / password / passwd / auth / bearer / credential
//!    （不区分大小写）时，其值被脱敏——无论写成 `--api-key value`、`--api-key=value`
//!    还是 `API_KEY=value`；`--api-key` 后面紧跟的那个参数同样按值脱敏；
//! 2. 值本身长得像令牌：以 `sk-`、`ghp_`、`xox` 开头，或长度 ≥ 32 的十六进制 / base64 串。
//!
//! 本模块只处理「要写给人看或存进工作空间的文本」。启动 Agent 仍用 `harnesses` 表里的原始参数：
//! 那是保留原值的唯一位置，除此之外任何表、任何文件都不留密钥（D6）。

/// 脱敏后的占位符。
const MASK: &str = "***";

/// 名字里出现这些片段就认为该参数的值是密钥（不区分大小写）。
const SENSITIVE_WORDS: &[&str] = &[
    "key",
    "token",
    "secret",
    "password",
    "passwd",
    "auth",
    "bearer",
    "credential",
];

/// 这些前缀开头的值按令牌处理。
const TOKEN_PREFIXES: &[&str] = &["sk-", "ghp_", "xox"];

/// 长随机串（十六进制 / base64）的最小长度。
const MIN_TOKEN_LEN: usize = 32;

/// 参数名去掉前导 `-` 并转小写后，是否是疑似密钥的名字。
fn is_sensitive_name(name: &str) -> bool {
    let name = name.trim().trim_start_matches('-').to_lowercase();
    SENSITIVE_WORDS.iter().any(|word| name.contains(word))
}

/// 值本身是否长得像令牌（与名字无关）。
///
/// 十六进制取长度 ≥ 32 的整串（Git 短哈希一类不会误伤）；base64 另要求字母与数字都有、且不含
/// `/`，这样路径（`src/a/b.rs`）和中文排版不会被当成令牌。
fn is_token_like(value: &str) -> bool {
    if value.len() < MIN_TOKEN_LEN {
        return value.len() >= 3 && {
            let lower = value.to_lowercase();
            TOKEN_PREFIXES.iter().any(|p| lower.starts_with(p))
        };
    }
    let hex = value.chars().all(|c| c.is_ascii_hexdigit());
    if hex {
        return true;
    }
    let alnum = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '=' | '-' | '_'));
    let mixed =
        value.chars().any(|c| c.is_ascii_alphabetic()) && value.chars().any(|c| c.is_ascii_digit());
    alnum && mixed
}

/// `name` 与 `value` 之间（或名字本身就是）的分隔符位置。
fn split_at_sep(token: &str) -> Option<usize> {
    token.find(['=', ':']).filter(|i| *i > 0)
}

/// 脱敏单个片段：三种写法（`--k=v`、`k=v`、`k: v`）与裸露的令牌都在这里识别。
///
/// 名字像密钥、或值本身像令牌时替换为 [`MASK`]，否则原样返回。
fn redact_token(token: &str) -> String {
    if token.is_empty() {
        return String::new();
    }
    // 整段就是令牌（含 base64 的 `=` 填充）时直接脱敏，不必再按分隔符拆开。
    if is_token_like(token) {
        return MASK.to_owned();
    }
    match split_at_sep(token) {
        Some(split) => {
            let (name, rest) = token.split_at(split);
            let (sep, value) = rest.split_at(1);
            let value = value.trim_matches(['"', '\'']);
            if value.is_empty() {
                return token.to_owned();
            }
            if is_sensitive_name(name) || is_token_like(value) {
                format!("{name}{sep}{MASK}")
            } else {
                token.to_owned()
            }
        }
        None => token.to_owned(),
    }
}

/// 脱敏一段任意文本：处理 `--key=value`、`KEY=value`、`key: value` 以及裸露的令牌。
///
/// 按空白与常见分隔符切分再逐段处理，保留分隔符本身，输出与输入的排版一致。
pub fn text(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(value.len());
    let mut start = 0;
    for (i, c) in value.char_indices() {
        if !(c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';' | '（' | '）')) {
            continue;
        }
        out.push_str(&redact_token(&value[start..i]));
        out.push(c);
        start = i + c.len_utf8();
    }
    out.push_str(&redact_token(&value[start..]));
    out
}

/// 脱敏启动参数数组。
///
/// `--api-key sk-xxx` 这种「名字与值分成两个参数」的写法要靠相邻关系识别：只有前一个参数名疑似
/// 密钥时，才把后一个不带 `-` 的参数整段脱敏，`--model gpt` 因此不受影响。
pub fn args(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut take_next = false;
    for arg in args {
        let mut piece = redact_token(arg);
        if take_next && !arg.starts_with('-') && !arg.is_empty() {
            piece = MASK.to_owned();
        }
        // 名字可疑、且值不在这一个参数里（`--api-key` 而非 `--api-key=x`）时，下一个参数按值脱敏。
        take_next = match split_at_sep(arg) {
            Some(_) => false,
            None => is_sensitive_name(arg),
        };
        out.push(piece);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    /// 名字含关键词：分开的值、`=` 写法、`:` 写法都要脱敏。
    #[test]
    fn masks_sensitive_names_in_every_form() {
        let got = args(&v(&[
            "--api-key",
            "sk-test-123",
            "--token=abc",
            "auth:xyz",
            "API_KEY=qwe",
            "--secret",
            "hunter2",
        ]));
        assert_eq!(
            got,
            v(&[
                "--api-key",
                "***",
                "--token=***",
                "auth:***",
                "API_KEY=***",
                "--secret",
                "***"
            ])
        );
        // 关键词的各种拼写与大小写都算。
        for name in [
            "--key",
            "--apiKey",
            "--TOKEN",
            "--passwd",
            "--password",
            "--bearer",
            "--credential",
            "--auth",
        ] {
            assert_eq!(args(&v(&[name, "zzz"]))[1], "***", "{name}");
        }
    }

    /// 常见令牌格式与长随机串即使不带关键词也要脱敏。
    #[test]
    fn masks_token_shaped_values() {
        for secret in [
            "sk-proj-AbCdEf0123456789",
            "ghp_0123456789abcdefghijklmnopqrstuvwxyz",
            "xoxb-1234567890-abcdefghij",
            "0123456789abcdef0123456789abcdef01234567",
            "c29tZS1sb25nLWJhc2U2NC10b2tlbi12YWx1ZS0xMjM0NTY3OA==",
        ] {
            assert_eq!(text(secret), "***", "{secret}");
            assert_eq!(args(&v(&["--flag", secret]))[1], "***", "{secret}");
            assert_eq!(
                text(&format!("boom: invalid api key {secret}")),
                "boom: invalid api key ***",
                "{secret}"
            );
        }
    }

    /// 名字不含关键词、但值是令牌时也要脱敏（Agent 的参数里常见这种写法）。
    #[test]
    fn masks_token_values_under_ordinary_names() {
        assert_eq!(text("leak=sk-test-123"), "leak=***");
        assert_eq!(args(&v(&["leak=sk-test-123"])), v(&["leak=***"]));
    }

    /// 正常参数不能被误伤。
    #[test]
    fn keeps_ordinary_arguments() {
        let want = v(&[
            "--model",
            "gpt",
            "--verbose",
            "acp",
            "--experimental-acp",
            "--experimental",
            "--max-turn-requests",
            "12",
            "Teamspace/前端",
            "mode=work",
            "--output",
            "/tmp/out.txt",
        ]);
        assert_eq!(args(&want.clone()), want);
    }

    /// 普通文本与中文排版保持原样。
    #[test]
    fn keeps_ordinary_text() {
        for keep in [
            "",
            "Agent 进程退出（signal: 9）",
            "超时：Agent 在 20 秒内没有响应 initialize",
            "执行失败：需要登录，请先完成登录",
            "改动文件：src/main.rs、README.md 共 3 个",
            "FE-001 第 1 次执行完成（Mock）",
            "工作空间（普通文件夹）正在执行 FE-002：同一文件夹同时只能运行一个执行",
            "Teamspace/前端/过程文件/素材清单.md",
        ] {
            assert_eq!(text(keep), keep, "{keep}");
        }
    }

    /// Agent 的 stderr 尾部：密钥被替换，前后文字不动。
    #[test]
    fn redacts_agent_stderr_tail() {
        assert_eq!(
            text("fatal: invalid api key sk-test-123"),
            "fatal: invalid api key ***"
        );
        assert_eq!(
            text("Error: TOKEN=abc123 expired"),
            "Error: TOKEN=*** expired"
        );
        assert_eq!(text("agent started ok"), "agent started ok");
    }

    /// 环境配置档案的 `KEY=VALUE` 值按同样规则处理，变量名与普通值不动。
    #[test]
    fn redacts_env_profile_values() {
        let vars =
            crate::run::env_profile::parse("API_KEY=sk-test-123\nMODEL=gpt\nPATH=/usr/bin\n")
                .unwrap();
        let redacted: Vec<(String, String)> =
            vars.into_iter().map(|(k, v)| (k, text(&v))).collect();
        assert_eq!(
            redacted,
            vec![
                ("API_KEY".to_owned(), "***".to_owned()),
                ("MODEL".to_owned(), "gpt".to_owned()),
                ("PATH".to_owned(), "/usr/bin".to_owned()),
            ]
        );
    }

    /// 引号包住的值也要脱敏，引号本身保留。
    #[test]
    fn redacts_quoted_values() {
        assert_eq!(text("--api-key=\"sk-abc\""), "--api-key=\"***\"");
        assert_eq!(text("--api-key='sk-abc'"), "--api-key='***'");
        assert_eq!(args(&v(&["--api-key", "sk-abc"]))[1], "***");
    }

    /// 脱敏后的文本里不应再出现原密钥，正常参数仍在。
    #[test]
    fn no_secret_survives_a_full_pass() {
        let got = args(&v(&[
            "--api-key",
            "sk-test-123",
            "leak=sk-test-123",
            "mode=stderrleak",
        ]));
        let joined = got.join(" ");
        assert!(!joined.contains("sk-test-123"), "{joined}");
        assert!(joined.contains("mode=stderrleak"), "{joined}");
    }
}
