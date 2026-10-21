// 执行相关的展示辅助：状态、失败类别、工具类型的中文标签与时间格式。
// 业务判断（能否重试、能否接管、失败原因）一律以后端返回的字段为准，这里只做文案与格式化。
import type { TagTone } from "../../../components/index.js";
import type { FailureKind } from "../../../bindings/FailureKind.js";
import type { FileChangeKind } from "../../../bindings/FileChangeKind.js";
import type { PermissionOption } from "../../../bindings/PermissionOption.js";
import type { Run } from "../../../bindings/Run.js";
import type { RunStatus } from "../../../bindings/RunStatus.js";
import type { TaskStatus } from "../../../bindings/TaskStatus.js";
import type { RunUpdate } from "../../../bindings/RunUpdate.js";
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

/**
 * 权限选项的按钮文案：按 kind 统一成中文，不直接显示 Agent 提供的 name（真实 Agent 多为英文或各自的名字）。
 * 未知 kind 回退到 Agent 提供的 name。用 switch 而不是对象查表，避免 kind 撞上 Object 原型上的属性。
 */
export function permissionOptionLabel(option: PermissionOption): string {
  switch (option.kind) {
    case "allow_once":
      return "允许一次";
    case "allow_always":
      return "本次执行内总是允许";
    case "reject_once":
      return "拒绝";
    case "reject_always":
      return "总是拒绝";
    default:
      return option.name;
  }
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

/**
 * 编辑、读取类请求的授权范围说明（与内核 run/grants.rs 一致）：「允许一次」只放行这一次写入或读取，
 * 「总是允许」在本次执行内持续有效；改写工作日志已有内容每次都要确认；读取授权不能用来写入（D18）。
 */
export function permissionScopeNote(
  toolKind: string | null,
  options: PermissionOption[],
): string | null {
  if (!options.some((option) => option.kind.startsWith("allow"))) {
    return null;
  }
  if (toolKind === "edit") {
    return "「允许一次」只放行这一次写入；「总是允许」在本次执行内对同一文件持续有效。改写工作日志已有内容每次都需要确认。";
  }
  if (toolKind === "read" || toolKind === "search") {
    return "「允许一次」只放行这一次读取；「总是允许」在本次执行内对这些路径持续有效。允许读取不等于允许写入。";
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

/**
 * 运行时长格式化成 `m:ss`（超过一小时 `h:mm:ss`）：心跳指示与执行信息条的「已运行」共用，
 * 分钟不补零，读起来更接近「已运行 3 分 12 秒」的说法。
 */
export function formatRunClock(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const rest = total % 60;
  const pad = (value: number) => String(value).padStart(2, "0");
  return hours > 0
    ? `${String(hours)}:${pad(minutes)}:${pad(rest)}`
    : `${String(minutes)}:${pad(rest)}`;
}

/** 相对时间的中文短语：不足 1 秒「刚刚」，之后按秒 / 分 / 小时。 */
export function formatSecondsAgo(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  if (total < 1) {
    return "刚刚";
  }
  if (total < 60) {
    return `${String(total)} 秒前`;
  }
  const minutes = Math.floor(total / 60);
  if (minutes < 60) {
    return `${String(minutes)} 分钟前`;
  }
  return `${String(Math.floor(minutes / 60))} 小时前`;
}

/**
 * 长时间没有新动静时的温和提醒（D23）：超过阈值给一句话说明可以取消或人工接管，
 * 不判定执行失败——Agent 可能确实在长时间思考。阈值内返回 null。
 */
export const RUN_STALE_SECONDS = 60;

export function runStaleHint(seconds: number): string | null {
  const total = Math.max(0, Math.floor(seconds));
  if (total < RUN_STALE_SECONDS) {
    return null;
  }
  const since =
    total < 120
      ? `${String(total)} 秒`
      : `${String(Math.floor(total / 60))} 分 ${String(total % 60)} 秒`;
  return `已 ${since}没有新的进度，Agent 可能在长时间思考；可取消或人工接管`;
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

/**
 * 执行目录的标题行文案。Git worktree 要带上分支名（`Git worktree · 分支 teamspace/XX-001-r1`），
 * 分支名由后端的 `run.branch` 提供；原地执行只说明是普通文件夹。
 */
export function execDirTitle(run: Run): string {
  return run.isolation === "worktree"
    ? `Git worktree · 分支 ${run.branch ?? "—"}`
    : "原地：普通文件夹";
}

/**
 * 执行状态的显示名。取后端返回的 `statusLabel`（后端已把人工接管统一成「已被接管」，
 * 接管后再送验收的验收页也用同一份文案）；这里只补胶囊的颜色。
 */
export function runDisplayStatus(run: Run): { label: string; tone: TagTone } {
  return {
    label: run.statusLabel,
    tone: run.takenOver ? "info" : runStatusTone[run.status],
  };
}

/** 成功但没有任何改动的提醒文案（后端 `run.noChanges` 为真时显示）。 */
export const NO_CHANGES_TEXT =
  "这次执行没有产生任何改动，Agent 可能提前结束（例如权限被拒）；建议退回并说明原因";

/** 提交哈希的短显示（与 git 默认一致的前 7 位）。 */
export function shortCommit(commit: string): string {
  return commit.slice(0, 7);
}

/** 中断时改动被保全到执行分支的说明；没有保全时为 `null`。 */
export function preservedChangesText(run: Run): string | null {
  if (run.interruptedCommit === null) {
    return null;
  }
  return `中断前的改动已保存在执行分支 ${run.branch ?? "—"} 的提交 ${shortCommit(run.interruptedCommit)}`;
}

/** 中断时没能把改动保全到执行分支的说明；保全成功或不需要保全时为 `null`。 */
export function preservedProblemText(run: Run): string | null {
  if (run.interruptedProblem === null) {
    return null;
  }
  return `中断时没能把改动提交到执行分支：${run.interruptedProblem}`;
}

/**
 * 中断后的续接能力说明：
 * - 有会话且 Agent 声明支持 loadSession → 「支持续接（loadSession）」；
 * - 会话都没建立 → 说明中断发生得太早；
 * - 其余 → Agent 不支持 loadSession。
 */
export function resumeCapabilityText(run: Run): string {
  if (run.canResume) {
    return "支持续接（loadSession）";
  }
  if (run.sessionId === null) {
    return "不支持续接（会话尚未建立）";
  }
  return "不支持续接（Agent 未提供 loadSession）";
}

/** 不支持续接时的原因说明，用于置灰按钮的 title。 */
export function noResumeReason(run: Run): string {
  if (run.sessionId === null) {
    return "这次执行在会话建立前就中断了，没有可续接的会话";
  }
  return `${run.harness.name} 不支持续接会话（loadSession）`;
}

/**
 * 中断前最后一条有意义的事件：最近一次工具调用或 Agent 消息摘要。
 * 只回看最后 6 条，避免为了一句摘要去翻整段长文本；没有则返回 null。
 */
export function lastEventSummary(updates: RunUpdate[]): string | null {
  for (const update of [...updates].reverse().slice(0, 6)) {
    const body = update.body;
    if (body.type === "tool_call") {
      return `${toolKindName(body.kind)}：${body.title}`;
    }
    if (body.type === "message" && body.role === "agent") {
      const line = body.text
        .split("\n")
        .map((item) => item.trim())
        .find((item) => item !== "");
      return line === undefined ? null : `Agent 消息：${line}`;
    }
  }
  return null;
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

/**
 * 成功的执行面板上是否还有「去验收」：执行还没有验收结果（待验收），或验收时合并冲突且任务仍因此受阻。
 * 已通过、已退回、冲突后被「重新执行」丢弃的执行（任务已不是受阻）不再有——以后端的 `Run.review`
 * 与任务状态为准。
 */
export function canGoReview(run: Run, taskStatus: string): boolean {
  if (run.review === null) {
    return true;
  }
  return run.review.outcome === "conflict" && taskStatus === "blocked";
}

/** 已有验收结果的成功执行在面板上的状态标签。 */
export function reviewedLabel(run: Run, taskStatus: string): string | null {
  if (run.review === null) {
    return null;
  }
  switch (run.review.outcome) {
    case "approved":
      return "已完成 · 已通过验收";
    case "rejected":
      return "已完成 · 已退回";
    case "conflict":
      return taskStatus === "blocked" ? "已完成 · 合并冲突" : "已完成 · 旧执行已丢弃";
  }
}

/**
 * 任务详情页头是否显示「开始执行 / 重新执行」。
 *
 * 已验收通过（done）的任务不再显示：内核里 done 只能回到 READY（`TaskStatus::next`），
 * runProblems 也会说明「只有 READY、受阻，或执行中断、人工接管后的任务可以开始执行」，
 * 这时页头只剩一个永远点不动的按钮。需要重做先在「状态操作」里重新打开为 READY，按钮会重新出现。
 *
 * 其余状态一律显示，可用与否仍以后端返回的 runProblems 为准：有问题时置灰并在 title 里说明原因。
 */
export function showStartRunButton(taskStatus: TaskStatus): boolean {
  return taskStatus !== "done";
}
