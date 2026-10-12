// 仅在 `vite --mode e2e` 下由 main.tsx 动态加载；正式构建不包含此文件。
// 把所有 invoke 转发给 crates/teamspace-e2e-bridge 的 HTTP 服务（真实内核），不使用任何假数据。
import { mockIPC } from "@tauri-apps/api/mocks";

interface BridgeReply {
  ok?: unknown;
  err?: unknown;
}

export function installBridge(): void {
  const bridge = import.meta.env.VITE_E2E_BRIDGE;
  if (typeof bridge !== "string" || bridge === "") {
    throw new Error("VITE_E2E_BRIDGE is required in e2e mode.");
  }
  mockIPC(async (cmd: string, args?: unknown) => {
    const response = await fetch(`${bridge}/invoke`, {
      method: "POST",
      headers: { "Content-Type": "text/plain" },
      body: JSON.stringify({ cmd, args: args ?? {} }),
    });
    const reply = (await response.json()) as BridgeReply;
    if (reply.err !== undefined) {
      // invoke 失败时以 ErrorPayload 对象拒绝，与真实 Tauri 一致。
      // eslint-disable-next-line @typescript-eslint/only-throw-error
      throw reply.err;
    }
    return reply.ok;
  });
}
