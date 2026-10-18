// 仅在 `vite --mode e2e` 下由 main.tsx 动态加载；正式构建不包含此文件。
// 把所有 invoke 转发给 crates/teamspace-e2e-bridge 的 HTTP 服务（真实内核），不使用任何假数据。
// 执行事件：桥接把内核推送的事件缓存起来，这里每 300ms 轮询一次（`__poll_run_events`），再用 mock 的
// 事件系统 emit 给 `listen` 的订阅者，页面代码与正式应用一致（订阅 RUN_EVENT）。
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";

import { RUN_EVENT } from "../api/commands.js";

interface BridgeReply {
  ok?: unknown;
  err?: unknown;
}

interface PolledEvents {
  events: { seq: number; event: unknown }[];
  next: number;
}

const POLL_INTERVAL_MS = 300;

/**
 * 只在端到端测试里打开的开关：在页面加载前用 page.addInitScript 把它设为 true，
 * `plugin:event|listen` 就会像真实应用里 capability 没放行事件权限（D23）那样被拒绝，
 * 用例据此验证「实时通道不可用时靠轮询兜底，界面照样跟着执行状态走」。
 */
const NO_REALTIME_FLAG = "__teamspace_no_realtime";

function realtimeBlocked(): boolean {
  const flag = (window as unknown as Record<string, unknown>)[NO_REALTIME_FLAG];
  return flag === true;
}

async function call(bridge: string, cmd: string, args: unknown): Promise<unknown> {
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
}

function pollRunEvents(bridge: string): void {
  // 首次只取当前序号：页面加载前的事件不重放。
  let after: number | null = null;
  const tick = async () => {
    try {
      const polled = (await call(bridge, "__poll_run_events", {
        after: after ?? 0,
      })) as PolledEvents;
      if (after !== null) {
        for (const item of polled.events) {
          await emit(RUN_EVENT, item.event);
        }
      }
      after = polled.next;
    } catch {
      // 桥接暂时不可用时下次再试。
    }
    setTimeout(() => void tick(), POLL_INTERVAL_MS);
  };
  void tick();
}

export function installBridge(): void {
  const bridge = import.meta.env.VITE_E2E_BRIDGE;
  if (typeof bridge !== "string" || bridge === "") {
    throw new Error("VITE_E2E_BRIDGE is required in e2e mode.");
  }
  const blocked = realtimeBlocked();
  mockIPC(
    (cmd: string, args?: unknown) => {
      // 关闭模拟事件时，事件相关命令落到这里并像 Tauri 的权限拒绝一样失败。
      if (blocked && cmd.startsWith("plugin:event|")) {
        return Promise.reject(
          new Error(
            `${cmd} not allowed. Permissions associated with this command: core:event:allow-listen, core:event:allow-unlisten`,
          ),
        );
      }
      return call(bridge, cmd, args);
    },
    // 打开时事件由 mock 的事件系统处理，不经过上面的回调。
    { shouldMockEvents: !blocked },
  );
  if (!blocked) {
    pollRunEvents(bridge);
  }
}
