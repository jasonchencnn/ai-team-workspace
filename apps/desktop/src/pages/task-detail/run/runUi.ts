// 执行相关的展示辅助：状态、失败类别、工具类型的中文标签与时间格式。
// 业务判断（能否重试、能否接管、失败原因）一律以后端返回的字段为准，这里只做文案与格式化。
import type { TagTone } from "../../../components/index.js";
import type { FailureKind } from "../../../bindings/FailureKind.js";
import type { FileChangeKind } from "../../../bindings/FileChangeKind.js";
import type { PermissionOption } from "../../../bindings/PermissionOption.js";
import type { Run } from "../../../bindings/Run.js";
import type { RunStatus } from "../../../bindings/RunStatus.js";
import type { RunUpdateBody } from "../../../bindings/RunUpdateBody.js";

/** 执行状态对应的胶囊颜色。 */
export const runStatusTone: Record<RunStatus, TagTone> = {
  queued: "info",
  running: "info",
  waiting_permission: "warning",
  succeeded: "success",
  failed: "danger",
  cancelled: "neutral",
  interrupted: "warning",
};

/**
 * 执行状态的显示名。Run 自带 statusLabel，这里补的是进度条目里的状态变化行
 * （RunUpdateBody.status 只有枚举，没有标签），取值与内核的 label() 一致。
 */
export const runStatusLabel: Record<RunStatus, string> = {
  queued: "准备中",
  running: "执行中",
  waiting_permission: "等待确认",
  succeeded: "已完成",
  failed: "失败",
  cancelled: "已取消",
  interrupted: "已中断",
};

/** 失败类别的中文标签（后端 `failureKind`）。 */
export const failureKindLabel: Record<FailureKind, string> = {
  needs_auth: "需要登录",
  agent_exited: "Agent 退出",
  timeout: "超时",
  protocol: "协议错误",
  refused: "拒绝执行",
  limit: "超出限制",
  launch: "启动失败",
  interrupted: "中断",
};

/** 失败类别对应的行动建议（提示用户下一步做什么）。 */
const failureKindAdvice: Record<FailureKind, string> = {
  needs_auth:
    "登录由 Harness 自己管理，Teamspace 不保存凭据。请在该 Harness 的终端里完成登录，再点「重试」。",
  agent_exited: "Agent 进程已退出，ACP 会话断开。可以点「重试」重新执行，或换绑其他 Harness。",
  timeout: "长时间没有收到新的输出，执行已自动停止。可以点「重试」，或改用响应更快的 Harness。",
  protocol: "与 Harness 的 ACP 会话不兼容。可以换绑其他 Harness，或改用「自定义 ACP Agent」接入。",
  refused: "Harness 拒绝了这次执行，通常是提示内容触发了它的拒绝规则。检查任务说明后重试。",
  limit: "触发了 Harness 的用量或轮次上限。等额度恢复后重试，或换绑其他 Harness。",
  launch: "没能启动 Harness 进程。到「Harness 接入」检查启动命令与参数是否正确。",
  interrupted: "应用退出或重启时执行仍在进行，Agent 进程已不在。可以续接会话或重新执行。",
};

/** 失败类别的行动建议文案。 */
export function failureAdvice(kind: FailureKind): string {
  return failureKindAdvice[kind];
}

/** 改动的类型标签。 */
export const fileChangeLabel: Record<FileChangeKind, string> = {
  added: "新增",
  modified: "已修改",
  deleted: "已删除",
};

/** 改动的类型的胶囊颜色。 */
export const fileChangeTone: Record<FileChangeKind, TagTone> = {
  added: "success",
  modified: "info",
  deleted: "danger",
};

/** ACP 工具类型的中文标签（执行区卡片与权限弹窗共用）。 */
const toolKindLabel: Record<string, string> = {
  read: "读取",
  edit: "编辑",
  delete: "删除",
  move: "移动",
  search: "搜索",
  execute: "执行命令",
  think: "思考",
  fetch: "联网",
  switch_mode: "切换模式",
  other: "其他",
};

/** 工具类型标签；Agent 未提供类型时按「操作」处理。 */
export function toolKindName(kind: string | null): string {
  if (kind === null || kind === "") {
    return "操作";
  }
  return toolKindLabel[kind] ?? kind;
}

/** 工具类型对应的卡片色调。 */
export function toolKindTone(kind: string | null): "info" | "warning" {
  return kind === "execute" ? "warning" : "info";
}

/** 工具调用状态的中文标签。 */
const toolStatusLabel: Record<string, string> = {
  pending: "等待中",
  in_progress: "进行中",
  completed: "已完成",
  failed: "失败",
};

/** 工具调用状态的胶囊颜色。 */
export function toolStatusTone(status: string | null): TagTone {
  switch (status) {
    case "completed":
      return "success";
    case "failed":
      return "danger";
    case "in_progress":
      return "info";
    default:
      return "neutral";
  }
}

/** 工具调用状态标签；Agent 未提供时不算显示。 */
export function toolStatusName(status: string | null): string | null {
  if (status === null || status === "") {
    return null;
  }
  return toolStatusLabel[status] ?? status;
}

/** 权限选项种类对应的按钮样式：允许为主按钮，拒绝为弱化按钮。 */
export function permissionOptionVariant(
  option: PermissionOption,
): "primary" | "neutral" | "subtle" {
  if (option.kind.startsWith("allow")) {
    return option.kind === "allow_once" ? "primary" : "neutral";
  }
  return "subtle";
}

/** 权限选项说明：区分「总是允许」只在本次执行内有效。 */
export function permissionOptionHint(option: PermissionOption): string | null {
  if (option.kind === "allow_always") {
    return "「总是允许」只在本次执行内有效，执行结束即失效。";
  }
  if (option.kind === "reject_always") {
    return "「总是拒绝」会让本次执行内同类操作都被拒绝。";
  }
  return null;
}

/** 计划项的状态标记。 */
export function planEntryMarker(status: string): string {
  switch (status) {
    case "completed":
      return "✓";
    case "in_progress":
      return "◉";
    default:
      return "○";
  }
}

/** 计划项优先级的标签；未声明时不显示。 */
export function planPriorityLabel(priority: string): string | null {
  switch (priority) {
    case "high":
      return "高";
    case "medium":
      return "中";
    case "low":
      return "低";
    default:
      return null;
  }
}

/** 消息来源的中文标签。 */
const messageRoleLabel: Record<string, string> = {
  agent: "Agent",
  thought: "思考",
  user: "你",
};

/** 消息来源标签。 */
export function messageRoleName(role: string): string {
  return messageRoleLabel[role] ?? role;
}

/** 把 UTC 时间转成本地可读时间；无法解析时原样返回。 */
export function formatLocal(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return iso;
  }
  return date.toLocaleString("zh-CN", { hour12: false });
}

/** 今天只显示时分，用于「今天 10:12 起」这类文案。 */
export function formatTimeOfDay(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return iso;
  }
  return date.toLocaleTimeString("zh-CN", { hour12: false, hour: "2-digit", minute: "2-digit" });
}

/** 相对时间（今天 / 昨天 / 日期），用于历次执行记录。 */
export function formatRelativeStart(iso: string, now: number = Date.now()): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return iso;
  }
  const startOfToday = new Date(now);
  startOfToday.setHours(0, 0, 0, 0);
  const days = Math.floor((startOfToday.getTime() - date.setHours(0, 0, 0, 0)) / 86_400_000);
  const clock = formatTimeOfDay(iso);
  if (days <= 0) {
    return `今天 ${clock}`;
  }
  if (days === 1) {
    return `昨天 ${clock}`;
  }
  return date.toLocaleDateString("zh-CN", { month: "numeric", day: "numeric" }) + ` ${clock}`;
}

/** 秒数格式化成 `MM:SS` 或 `HH:MM:SS`。 */
export function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const rest = total % 60;
  const pad = (value: number) => String(value).padStart(2, "0");
  return hours > 0
    ? `${String(hours)}:${pad(minutes)}:${pad(rest)}`
    : `${pad(minutes)}:${pad(rest)}`;
}

/** 已运行的秒数：已结束用 endedAt - startedAt，进行中用当前时间 - startedAt。 */
export function runElapsedSeconds(run: Run, now: number = Date.now()): number {
  const start = new Date(run.startedAt).getTime();
  if (Number.isNaN(start)) {
    return 0;
  }
  const end = run.endedAt === null ? now : new Date(run.endedAt).getTime();
  if (Number.isNaN(end)) {
    return 0;
  }
  return Math.max(0, Math.round((end - start) / 1000));
}

/** 执行是否已结束（用于停止「已运行」计时）。 */
export function hasEnded(run: Run): boolean {
  return run.endedAt !== null;
}

/** 执行目录的显示文案：原地执行说明是普通文件夹。 */
export function execDirLabel(run: Run): string {
  return run.isolation === "worktree"
    ? `Worktree：${run.execDir}`
    : `原地：普通文件夹 ${run.execDir}`;
}

/** Harness 的显示文案：预设标签 + ACP。 */
export function runHarnessLabel(run: Run): string {
  return run.harness.name;
}

/** 从进度条目里取出工具调用卡片需要的字段；不是工具调用时返回 null。 */
export function asToolCall(
  body: RunUpdateBody,
): Extract<RunUpdateBody, { type: "tool_call" }> | null {
  return body.type === "tool_call" ? body : null;
}

/** 任务卡片上的执行小标。 */
export interface RunBadge {
  label: string;
  tone: TagTone;
}

/**
 * 任务卡片 / 工作台上的执行小标：等待确认优先（需要 Human 介入），其次是执行中。
 * 其他状态（成功、失败、取消、中断）不在卡片上重复展示，任务状态本身已经说明。
 */
export function runBadge(run: Run | undefined): RunBadge | null {
  if (run === undefined) {
    return null;
  }
  if (run.status === "waiting_permission") {
    return { label: "等待你确认", tone: "warning" };
  }
  if (run.status === "queued" || run.status === "running") {
    return { label: `执行中 · ${run.harness.name}`, tone: "info" };
  }
  return null;
}
