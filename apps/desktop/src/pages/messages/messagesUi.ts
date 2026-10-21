// 岗位通信页共用的展示辅助：状态名与颜色、发送方名、时间格式、筛选参数与轮询判定。
// 状态名与后端一致（crates/teamspace-core/src/model/message.rs 的 label()），
// 页面不另造一套状态规则；能不能重发、要不要继续刷新都看后端返回的 status。
import type { TagTone } from "../../components/index.js";
import type { Message } from "../../bindings/Message.js";
import type { MessageSender } from "../../bindings/MessageSender.js";
import type { MessageStatus } from "../../bindings/MessageStatus.js";
import type { MessageThread } from "../../bindings/MessageThread.js";
import type { ThreadParticipant } from "../../bindings/ThreadParticipant.js";

/** 消息状态的中文名。写成 Record：后端新增状态时这里会直接编译失败，避免漏译。 */
export const MESSAGE_STATUS_LABELS: Record<MessageStatus, string> = {
  queued: "排队中",
  delivering: "投递中",
  replied: "已回复",
  failed: "失败",
  expired: "已过期",
};

/** 状态标签的颜色；颜色始终配文字，不单靠颜色表达状态。 */
export const MESSAGE_STATUS_TONES: Record<MessageStatus, TagTone> = {
  queued: "neutral",
  delivering: "info",
  replied: "success",
  failed: "danger",
  expired: "warning",
};

/** 发送方的身份名（岗位的具体名字由岗位列表给出）。 */
export const SENDER_LABELS: Record<MessageSender, string> = {
  human: "Human",
  leader: "Leader",
  role: "岗位",
};

/** 还在进行中：排队等待投递、已交给接收岗位的 Harness。 */
export function isPendingStatus(status: MessageStatus): boolean {
  return status === "queued" || status === "delivering";
}

/** 失败或过期：保留原消息，可以重新发送。 */
export function canRetry(status: MessageStatus): boolean {
  return status === "failed" || status === "expired";
}

/** 线程里最后一条消息（消息按时间正序返回）。 */
export function latestMessage(messages: Message[]): Message | null {
  return messages.length === 0 ? null : (messages[messages.length - 1] ?? null);
}

/** 有排队中或投递中的消息时，界面要继续刷新（等投递结果）。 */
export function shouldPollMessages(messages: Message[]): boolean {
  return messages.some((message) => isPendingStatus(message.status));
}

/**
 * 线程列表里还有排队中或投递中的消息时，界面要继续刷新。
 *
 * 列表直接带最近一条消息的状态（`lastStatus`），不必再逐个取线程详情。
 */
export function shouldPollThreads(threads: MessageThread[]): boolean {
  return threads.some((thread) => isPendingStatus(thread.lastStatus));
}

/**
 * 发送方显示名。
 *
 * 岗位发出的消息里 `senderRoleId` 是发信岗位（内核已记录），没有时退回接收岗位的名字。
 */
export function senderName(message: Message, roleNames: Map<string, string>): string {
  if (message.sender === "human" || message.sender === "leader") {
    return SENDER_LABELS[message.sender];
  }
  const roleId = message.senderRoleId ?? message.toRoleId;
  return roleNames.get(roleId) ?? SENDER_LABELS.role;
}

/** 线程参与方的显示名（去重，顺序按后端给出的顺序）。 */
export function participantNames(participants: ThreadParticipant[]): string[] {
  const names: string[] = [];
  for (const participant of participants) {
    if (!names.includes(participant.name)) {
      names.push(participant.name);
    }
  }
  return names;
}

/** 参与方串成一行（发送方与接收方之间用箭头）。 */
export function participantsLabel(participants: ThreadParticipant[]): string {
  return participantNames(participants).join(" ↔ ");
}

/** 列表里最多显示多少个线程：每个线程都要再取一次详情，太多会拖慢页面。 */
export const MAX_LISTED_THREADS = 50;

/** 页面筛选参数：岗位、任务、当前线程。 */
export const THREAD_PARAM = "thread";
export const ROLE_PARAM = "role";
export const TASK_PARAM = "task";

export interface ThreadFilters {
  /** 只看某岗位参与的线程。 */
  roleId: string;
  /** 只看关联某任务的线程。 */
  taskId: string;
}

/** 从地址栏读出筛选条件；无法识别的取值按不过滤处理。 */
export function parseThreadFilters(params: URLSearchParams): ThreadFilters {
  return {
    roleId: params.get(ROLE_PARAM) ?? "",
    taskId: params.get(TASK_PARAM) ?? "",
  };
}

/** 把筛选条件写回地址栏（空值不写，保持链接干净）。 */
export function serializeThreadFilters(filters: ThreadFilters): URLSearchParams {
  const params = new URLSearchParams();
  if (filters.roleId !== "") {
    params.set(ROLE_PARAM, filters.roleId);
  }
  if (filters.taskId !== "") {
    params.set(TASK_PARAM, filters.taskId);
  }
  return params;
}

function startOfLocalDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

const clock = (date: Date): string =>
  `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;

/**
 * 列表里的时间：今天的记「今天 10:31」，昨天记「昨天 17:40」，更早记「9/21 17:40」。
 * 与活动与审计页的口径一致，按本地时区判断归属日。
 */
export function formatMessageTime(iso: string, now: Date): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return iso;
  }
  const today = startOfLocalDay(now).getTime();
  const day = date.getTime() - (date.getHours() * 60 + date.getMinutes()) * 60_000;
  const diffDays = Math.round((today - day) / 86_400_000);
  if (diffDays === 0) {
    return `今天 ${clock(date)}`;
  }
  if (diffDays === 1) {
    return `昨天 ${clock(date)}`;
  }
  return `${String(date.getMonth() + 1)}/${String(date.getDate())} ${clock(date)}`;
}

/** 线程列表项的摘要：后端给的最近一条消息摘要（正文压成一行，超长截断）。 */
export function threadSnippet(thread: MessageThread): string {
  return thread.lastSnippet ?? "";
}

/** 线程在列表里的排序：最近更新的在前（后端已排序，这里只做兜底）。 */
export function byUpdatedDesc(threads: MessageThread[]): MessageThread[] {
  return [...threads].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
}
