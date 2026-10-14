//! 会话权限模式（M2.1-02，S-1a）：让 Agent 执行命令、写文件之前向 Client 请求权限。
//!
//! 背景：很多 ACP Agent 在会话里有「权限模式」，不同模式下是否发 `session/request_permission` 差别很大。
//! 例如 MiniMax Code 的 ACP 会话默认是 Full access（`bypassPermissions`），命令直接执行、从不请求；
//! Claude Code 适配器和 Gemini CLI 默认是 `default`（会请求），但用户自己的配置可能改成放行。
//! 权限规则（D15）由 Teamspace 在 `session/request_permission` 上执行，所以会话必须处在「会请求」的模式。
//!
//! 做法：`session/new` / `session/load` 的响应里，Agent 会公布可用的模式，有两处：
//! - `modes`（ACP 会话模式）：`currentModeId` + `availableModes[].id`，用 `session/set_mode` 切换；
//! - `configOptions`（ACP 会话配置项）里与权限相关的 `select` 项：用 `session/set_config_option` 切换。
//!
//! [`plan`] 只做判断（纯函数，便于测试）：若当前处在放行类模式，且 Agent 公布了「会请求」的模式，就返回要执行的切换，
//! 选最严格的「会请求」模式（`default` / `ask` 一类）。**永远不选** bypass / yolo / auto / acceptEdits 这类放行模式，
//! 也不选 plan / dontAsk 这类「根本不执行」的模式。找不到可切换的模式、或用户自己的启动参数已指定权限，
//! 且当前模式放行时，返回一条中文提示，由执行引擎写入执行记录。

use serde_json::Value;

/// 一个模式 ID 的类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeClass {
    /// 遇到需要批准的操作会向 Client 请求（`default`、`ask` 等）。
    Ask,
    /// 不执行或只读（`plan`、`dontAsk`、`readOnly` 的规划类模式等）：安全，但无法完成工作，不作为切换目标。
    Restricted,
    /// 不经确认就执行（`bypassPermissions`、`yolo`、`auto`、`acceptEdits`、`full access` 等）。
    Permissive,
    /// 无法判断（Agent 自定义的工作流模式等）。
    Unknown,
}

/// 「会请求」类模式，按偏好从严到宽排列（规范化后精确匹配）。
const ASK_PREFERENCE: [&str; 8] = [
    "default",
    "ask",
    "alwaysask",
    "manual",
    "confirm",
    "normal",
    "suggest",
    // Codex 适配器：只读沙箱，写入与命令都要批准。
    "readonly",
];
/// 包含这些片段的模式不执行或只读。
const RESTRICTED_PARTS: [&str; 5] = ["plan", "dontask", "deny", "never", "review"];
/// 包含这些片段的模式放行（`readonly` 已在上面精确匹配，不会走到这里）。
const PERMISSIVE_PARTS: [&str; 10] = [
    "bypass",
    "yolo",
    "auto",
    "accept",
    "full",
    "danger",
    "unrestricted",
    "trust",
    "allowall",
    "skip",
];

/// 小写并去掉分隔符，`bypassPermissions` / `bypass-permissions` / `bypass_permissions` 视为同一个。
fn normalize(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// 判断一个模式 ID 的类别。
pub fn classify(id: &str) -> ModeClass {
    let norm = normalize(id);
    if ASK_PREFERENCE.contains(&norm.as_str()) {
        return ModeClass::Ask;
    }
    if RESTRICTED_PARTS.iter().any(|p| norm.contains(p)) {
        return ModeClass::Restricted;
    }
    if PERMISSIVE_PARTS.iter().any(|p| norm.contains(p)) {
        return ModeClass::Permissive;
    }
    ModeClass::Unknown
}

/// 在候选里选最严格的「会请求」模式：按 [`ASK_PREFERENCE`] 的顺序。
fn strictest_ask(candidates: &[String]) -> Option<&String> {
    ASK_PREFERENCE
        .iter()
        .find_map(|want| candidates.iter().find(|c| normalize(c) == *want))
}

/// 一次要对 Agent 做的切换。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Switch {
    /// `session/set_mode`。
    Mode { mode_id: String },
    /// `session/set_config_option`。
    Config { config_id: String, value: String },
}

/// 对一个会话的处理计划。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub switches: Vec<Switch>,
    /// 当前模式放行、又无法（或不应）切换时的提示，写入执行记录。
    pub warnings: Vec<String>,
}

/// 用户自己的启动参数里是否已经指定了权限相关的选项（此时用户优先，Teamspace 不改模式，只提示）。
pub fn args_set_permissions(args: &[String]) -> bool {
    const HINTS: [&str; 10] = [
        "yolo",
        "permission",
        "approval",
        "bypass",
        "dangerous",
        "fullauto",
        "autoapprove",
        "allowall",
        "trustall",
        "skippermissions",
    ];
    args.iter().filter(|a| a.starts_with('-')).any(|a| {
        let norm = normalize(a);
        HINTS.iter().any(|h| norm.contains(h))
    })
}

/// 一个可切换的控件（模式或配置项）。
struct Control {
    /// 给人看的名称，用在提示里。
    label: String,
    current: String,
    values: Vec<String>,
    switch: fn(&str, &str) -> Switch,
    /// 控件标识（配置项 ID；模式为空）。
    id: String,
}

fn controls(response: &Value) -> Vec<Control> {
    let mut out = Vec::new();
    if let Some(modes) = response.get("modes").filter(|m| m.is_object()) {
        let current = modes
            .get("currentModeId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let values: Vec<String> = modes
            .get("availableModes")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        if !current.is_empty() {
            out.push(Control {
                label: "会话模式".to_owned(),
                current,
                values,
                switch: |_, value| Switch::Mode {
                    mode_id: value.to_owned(),
                },
                id: String::new(),
            });
        }
    }
    if let Some(options) = response.get("configOptions").and_then(Value::as_array) {
        for option in options {
            if option.get("type").and_then(Value::as_str) != Some("select") {
                continue;
            }
            let id = option.get("id").and_then(Value::as_str).unwrap_or_default();
            let category = option
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let norm_id = normalize(id);
            let permission_like = category == "_permission"
                || category == "mode"
                || norm_id.contains("permission")
                || norm_id.contains("approval");
            if id.is_empty() || !permission_like {
                continue;
            }
            let Some(current) = option.get("currentValue").and_then(Value::as_str) else {
                continue;
            };
            out.push(Control {
                label: option
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|n| !n.is_empty())
                    .unwrap_or(id)
                    .to_owned(),
                current: current.to_owned(),
                values: option_values(option.get("options")),
                switch: |id, value| Switch::Config {
                    config_id: id.to_owned(),
                    value: value.to_owned(),
                },
                id: id.to_owned(),
            });
        }
    }
    out
}

/// 配置项的取值：扁平列表 `[{value}]` 或分组 `[{options: [{value}]}]`。
fn option_values(options: Option<&Value>) -> Vec<String> {
    let mut values = Vec::new();
    for item in options.and_then(Value::as_array).into_iter().flatten() {
        if let Some(value) = item.get("value").and_then(Value::as_str) {
            values.push(value.to_owned());
        } else if let Some(group) = item.get("options") {
            values.extend(option_values(Some(group)));
        }
    }
    values
}

/// 根据 `session/new` / `session/load` 的响应，决定要做哪些切换。
///
/// `user_controls`：用户自己的启动参数已指定权限，不切换，当前模式放行时只提示。
pub fn plan(response: &Value, user_controls: bool) -> Plan {
    let mut plan = Plan::default();
    for control in controls(response) {
        if classify(&control.current) != ModeClass::Permissive {
            continue;
        }
        let target = if user_controls {
            None
        } else {
            strictest_ask(&control.values)
        };
        match target {
            Some(value) => plan.switches.push((control.switch)(&control.id, value)),
            None => plan.warnings.push(warning(&control, user_controls)),
        }
    }
    plan
}

/// 切换失败时的提示。
pub fn switch_failed(switch: &Switch, reason: &str) -> String {
    let what = match switch {
        Switch::Mode { mode_id } => format!("会话模式「{mode_id}」"),
        Switch::Config { config_id, value } => format!("权限设置 {config_id}「{value}」"),
    };
    format!("无法把 Agent 切换到{what}（{reason}）。该 Agent 当前模式可能不经确认就执行操作。")
}

fn warning(control: &Control, user_controls: bool) -> String {
    let why = if user_controls {
        "启动参数已指定权限，Teamspace 保持不变"
    } else {
        "该 Agent 没有公布「执行前请求确认」的模式，Teamspace 无法切换"
    };
    format!(
        "该 Agent 当前模式可能不经确认就执行操作：{}为「{}」，{why}。命令和 shell 写文件不会弹出确认。",
        control.label, control.current
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn classifies_known_mode_ids() {
        for id in ["default", "ask", "Ask", "manual"] {
            assert_eq!(classify(id), ModeClass::Ask, "{id}");
        }
        for id in ["read-only", "readOnly"] {
            assert_eq!(classify(id), ModeClass::Ask, "{id}");
        }
        for id in ["plan", "dontAsk", "dont_ask"] {
            assert_eq!(classify(id), ModeClass::Restricted, "{id}");
        }
        for id in [
            "bypassPermissions",
            "yolo",
            "auto",
            "autoEdit",
            "acceptEdits",
            "full-access",
            "dangerously-skip-permissions",
        ] {
            assert_eq!(classify(id), ModeClass::Permissive, "{id}");
        }
        assert_eq!(classify("build"), ModeClass::Unknown);
    }

    #[test]
    fn minimax_style_permission_option_is_switched_to_ask() {
        // MiniMax Code 0.6.8 在 session/new 里的真实形状。
        let response = json!({
            "sessionId": "s",
            "modes": { "currentModeId": "default", "availableModes": [
                { "id": "default" }, { "id": "plan" }] },
            "configOptions": [
                { "type": "select", "id": "permissionMode", "name": "Permission mode",
                  "category": "_permission", "currentValue": "bypassPermissions",
                  "options": [
                      { "value": "default", "name": "Ask" },
                      { "value": "auto", "name": "Auto" },
                      { "value": "bypassPermissions", "name": "Full access" }] },
                { "type": "select", "id": "model", "category": "model", "currentValue": "m1",
                  "options": [{ "value": "m1" }] }
            ]
        });
        let plan = plan(&response, false);
        assert_eq!(
            plan.switches,
            [Switch::Config {
                config_id: "permissionMode".into(),
                value: "default".into()
            }]
        );
        assert!(plan.warnings.is_empty());
    }

    #[test]
    fn permissive_session_mode_picks_the_strictest_asking_mode() {
        let response = json!({ "modes": { "currentModeId": "bypassPermissions", "availableModes": [
            { "id": "bypassPermissions" }, { "id": "plan" }, { "id": "acceptEdits" },
            { "id": "dontAsk" }, { "id": "default" }] } });
        assert_eq!(
            plan(&response, false).switches,
            [Switch::Mode {
                mode_id: "default".into()
            }]
        );
        // Gemini 风格的 ID。
        let gemini = json!({ "modes": { "currentModeId": "yolo", "availableModes": [
            { "id": "default" }, { "id": "autoEdit" }, { "id": "yolo" }, { "id": "plan" }] } });
        assert_eq!(
            plan(&gemini, false).switches,
            [Switch::Mode {
                mode_id: "default".into()
            }]
        );
    }

    #[test]
    fn already_asking_or_restricted_sessions_are_left_alone() {
        for current in ["default", "plan", "dontAsk", "build"] {
            let response = json!({ "modes": { "currentModeId": current, "availableModes": [
                { "id": "default" }, { "id": "yolo" }] } });
            assert_eq!(plan(&response, false), Plan::default(), "{current}");
        }
        assert_eq!(plan(&json!({ "sessionId": "s" }), false), Plan::default());
    }

    #[test]
    fn never_switches_to_a_permissive_or_non_executing_mode() {
        // 公布的模式里没有「会请求」的：不切换，只提示。
        let response = json!({ "modes": { "currentModeId": "yolo", "availableModes": [
            { "id": "yolo" }, { "id": "plan" }, { "id": "acceptEdits" }, { "id": "dontAsk" }] } });
        let plan = plan(&response, false);
        assert!(plan.switches.is_empty());
        assert_eq!(plan.warnings.len(), 1);
        assert!(plan.warnings[0].contains("可能不经确认就执行操作"));
        assert!(plan.warnings[0].contains("yolo"));
    }

    #[test]
    fn user_arguments_win_but_a_permissive_mode_is_still_reported() {
        let response = json!({ "modes": { "currentModeId": "yolo", "availableModes": [
            { "id": "default" }, { "id": "yolo" }] } });
        let plan = plan(&response, true);
        assert!(plan.switches.is_empty());
        assert_eq!(plan.warnings.len(), 1);
        assert!(plan.warnings[0].contains("启动参数已指定权限"));
        // 用户参数下当前已是 default：什么也不说。
        let asking = json!({ "modes": { "currentModeId": "default", "availableModes": [
            { "id": "default" }, { "id": "yolo" }] } });
        assert_eq!(super::plan(&asking, true), Plan::default());
    }

    #[test]
    fn grouped_config_options_are_flattened() {
        let response = json!({ "configOptions": [
            { "type": "select", "id": "approval_mode", "currentValue": "full-access",
              "options": [{ "group": "g", "name": "G", "options": [
                  { "value": "full-access" }, { "value": "read-only" }] }] }] });
        assert_eq!(
            plan(&response, false).switches,
            [Switch::Config {
                config_id: "approval_mode".into(),
                value: "read-only".into()
            }]
        );
    }

    #[test]
    fn recognizes_permission_flags_in_user_arguments() {
        let args = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(args_set_permissions(&args(&[
            "--acp",
            "--permission-mode",
            "plan"
        ])));
        assert!(args_set_permissions(&args(&["--yolo"])));
        assert!(args_set_permissions(&args(&["--approval-mode=auto_edit"])));
        assert!(!args_set_permissions(&args(&["--acp", "--model", "x"])));
        assert!(!args_set_permissions(&args(&["acp"])));
        assert!(!args_set_permissions(&[]));
    }
}
