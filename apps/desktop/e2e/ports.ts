// 并行在多个 worktree 里跑 e2e 时，用 TEAMSPACE_E2E_PORT_BASE 错开端口。
const base = Number(process.env.TEAMSPACE_E2E_PORT_BASE ?? "14781");

export const BRIDGE_PORT = base;
export const UI_PORT = base + 1;
