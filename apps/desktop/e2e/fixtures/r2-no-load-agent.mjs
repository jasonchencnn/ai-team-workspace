// QA R2 用的最小 ACP Agent：握手时声明不支持 loadSession；收到提示后发一条消息并一直等待，
// 直到收到 session/cancel。用来制造「不支持续接」的中断执行（Mock Agent 总是支持续接）。
import { createInterface } from "node:readline";

const send = (message) => {
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);
};

let promptId = null;
const lines = createInterface({ input: process.stdin });
lines.on("line", (line) => {
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    return;
  }
  if (message.method === "initialize") {
    send({
      id: message.id,
      result: {
        protocolVersion: 1,
        agentCapabilities: { loadSession: false },
        authMethods: [],
        agentInfo: { name: "r2-no-load-agent", version: "1" },
      },
    });
  } else if (message.method === "session/new") {
    send({ id: message.id, result: { sessionId: "r2-session-1" } });
  } else if (message.method === "session/prompt") {
    promptId = message.id;
    send({
      method: "session/update",
      params: {
        sessionId: "r2-session-1",
        update: {
          sessionUpdate: "agent_message_chunk",
          content: { type: "text", text: "收到，正在长时间处理。" },
        },
      },
    });
  } else if (message.method === "session/cancel") {
    if (promptId !== null) {
      send({ id: promptId, result: { stopReason: "cancelled" } });
      promptId = null;
    }
  } else if (message.id !== undefined && message.method !== undefined) {
    send({
      id: message.id,
      error: { code: -32601, message: `Method not found: ${message.method}` },
    });
  }
});
