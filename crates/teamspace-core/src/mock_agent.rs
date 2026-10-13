//! 内置 Mock ACP Agent：用于测试和演示，不调用任何模型、不消耗额度。
//!
//! 宿主程序（桌面应用、端到端测试桥接）收到命令行参数 [`MOCK_AGENT_FLAG`] 时调用 [`run_stdio`]，
//! 以 ACP Agent 身份在 stdin / stdout 上收发按行分隔的 JSON-RPC 消息；内核通过
//! [`crate::api::HostConfig::mock_agent`] 得到启动它的命令。
//!
//! 支持的方法：
//! - `initialize`：返回协议版本 1、`agentInfo.name = teamspace-mock-agent`、全部能力为 `false`、
//!   无登录方式；
//! - `authenticate`：直接成功（Mock 不需要登录）；
//! - `session/new`（为 M2 预留）：返回新的会话 ID；
//! - `session/prompt`（为 M2 预留）：先发一条 `session/update`（`agent_message_chunk`）回显提示中的
//!   文字，再以 `end_turn` 结束；
//! - `session/cancel` 通知：忽略。
//!
//! 其他请求回复「方法不存在」，无法解析的行回复「解析错误」；stdin 关闭时退出。

use std::io::{BufRead, Write};

use serde_json::{Value, json};

/// 宿主程序以 Mock Agent 身份运行的命令行参数。
pub const MOCK_AGENT_FLAG: &str = "--teamspace-mock-agent";
/// Mock Agent 在 `agentInfo` 中报告的名称，也是 Mock 预设显示的启动命令。
pub const AGENT_NAME: &str = "teamspace-mock-agent";

/// 在当前进程的 stdin / stdout 上运行 Mock Agent，直到 stdin 关闭。
pub fn run_stdio() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    run(stdin.lock(), stdout.lock())
}

/// 在给定的输入输出上运行 Mock Agent（便于测试），直到输入结束。
pub fn run(input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    let mut sessions = 0u64;
    for line in input.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            write(
                &mut output,
                &json!({ "jsonrpc": "2.0", "id": null,
                         "error": { "code": -32700, "message": "Parse error" } }),
            )?;
            continue;
        };
        let method = message.get("method").and_then(Value::as_str);
        let Some(id) = message.get("id").filter(|id| !id.is_null()).cloned() else {
            // 通知（如 session/cancel）或 Client 的响应：不回复。
            continue;
        };
        let Some(method) = method else {
            continue;
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let reply = match method {
            "initialize" => ok(id, initialize_result()),
            "authenticate" => ok(id, json!({})),
            "session/new" => {
                sessions += 1;
                ok(
                    id,
                    json!({ "sessionId": format!("mock-session-{sessions}") }),
                )
            }
            "session/prompt" => {
                let session_id = params.get("sessionId").cloned().unwrap_or(Value::Null);
                let text = prompt_text(&params);
                write(
                    &mut output,
                    &json!({
                        "jsonrpc": "2.0",
                        "method": "session/update",
                        "params": {
                            "sessionId": session_id,
                            "update": {
                                "sessionUpdate": "agent_message_chunk",
                                "content": { "type": "text", "text": format!("Mock Agent 收到：{text}") }
                            }
                        }
                    }),
                )?;
                ok(id, json!({ "stopReason": "end_turn" }))
            }
            other => json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32601, "message": format!("Method not found: {other}") }
            }),
        };
        write(&mut output, &reply)?;
    }
    Ok(())
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": 1,
        "agentCapabilities": {
            "loadSession": false,
            "promptCapabilities": { "image": false, "audio": false, "embeddedContext": false },
            "mcpCapabilities": { "http": false, "sse": false }
        },
        "authMethods": [],
        "agentInfo": {
            "name": AGENT_NAME,
            "title": "Teamspace Mock Agent",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

/// 提示中所有文本块拼接起来的内容。
fn prompt_text(params: &Value) -> String {
    params
        .get("prompt")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn write(output: &mut impl Write, message: &Value) -> std::io::Result<()> {
    output.write_all(message.to_string().as_bytes())?;
    output.write_all(b"\n")?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::initialize_request;

    fn exchange(input: &str) -> Vec<Value> {
        let mut out = Vec::new();
        run(input.as_bytes(), &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn answers_initialize_with_agent_info() {
        let replies = exchange(&format!("{}\n", initialize_request()));
        assert_eq!(replies.len(), 1);
        let result = &replies[0]["result"];
        assert_eq!(result["protocolVersion"], 1);
        assert_eq!(result["agentInfo"]["name"], AGENT_NAME);
        assert_eq!(result["authMethods"], json!([]));
        let hs = crate::acp::interpret_response(&replies[0]);
        assert_eq!(hs.status, crate::model::HandshakeStatus::Connected);
    }

    #[test]
    fn echoes_prompts_in_sessions_and_rejects_unknown_methods() {
        let input = [
            r#"{"jsonrpc":"2.0","id":1,"method":"session/new","params":{"cwd":"/tmp","mcpServers":[]}}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"session/prompt","params":{"sessionId":"mock-session-1","prompt":[{"type":"text","text":"你好"}]}}"#,
            r#"{"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":"mock-session-1"}}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"session/load","params":{}}"#,
            "not json",
        ]
        .join("\n");
        let replies = exchange(&input);
        assert_eq!(replies[0]["result"]["sessionId"], "mock-session-1");
        assert_eq!(replies[1]["method"], "session/update");
        assert_eq!(
            replies[1]["params"]["update"]["content"]["text"],
            "Mock Agent 收到：你好"
        );
        assert_eq!(replies[2]["result"]["stopReason"], "end_turn");
        assert_eq!(replies[3]["error"]["code"], -32601);
        assert_eq!(replies[4]["error"]["code"], -32700);
        assert_eq!(replies.len(), 5);
    }
}
