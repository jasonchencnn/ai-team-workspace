// 待我处理、决策与通知订阅的界面逻辑。纯函数，便于单测；不碰后端规则。
import type { InboxCategory } from "../../bindings/InboxCategory.js";
import type { InboxCounts } from "../../bindings/InboxCounts.js";
import type { InboxItem } from "../../bindings/InboxItem.js";
import type { InboxTarget } from "../../bindings/InboxTarget.js";
import type { Notification } from "../../bindings/Notification.js";
import type { NotificationCategory } from "../../bindings/NotificationCategory.js";
import type { NotificationCategorySetting } from "../../bindings/NotificationCategorySetting.js";
import type { NotificationCategoryUpdate } from "../../bindings/NotificationCategoryUpdate.js";
import type { NotificationSettings } from "../../bindings/NotificationSettings.js";

/** 类别顺序：与后端 `InboxCategory::ALL` 一致（后端按这个顺序出事项）。 */
export const INBOX_CATEGORY_ORDER: readonly InboxCategory[] = [
  "review",
  "blocked",
  "permission",
  "decision",
  "leader_denied",
];

/** 类别兜底名：后端给了 `categoryLabel` 时用后端的，这里只是兜底。 */
export const INBOX_CATEGORY_FALLBACK_LABEL: Record<InboxCategory, string> = {
  review: "待验收",
  blocked: "执行失败 / 受阻",
  permission: "待答复权限",
  decision: "需要拍板的消息",
  leader_denied: "Leader 越权",
};

/** 一组待我处理事项。 */
export interface InboxGroup {
  category: InboxCategory;
  label: string;
  items: InboxItem[];
}

/**
 * 按类别分组：类别顺序固定，组内保持后端给出的顺序（时间倒序）。
 * 空类别不出现。
 */
export function groupInboxItems(items: readonly InboxItem[]): InboxGroup[] {
  const groups: InboxGroup[] = [];
  for (const category of INBOX_CATEGORY_ORDER) {
    const groupItems = items.filter((item) => item.category === category);
    if (groupItems.length === 0) {
      continue;
    }
    const first = groupItems[0];
    groups.push({
      category,
      label: (first?.categoryLabel ?? "") || INBOX_CATEGORY_FALLBACK_LABEL[category],
      items: groupItems,
    });
  }
  return groups;
}

/** 按类别计数。与后端 `InboxCounts` 同形，用于本地汇总（后端计数为准）。 */
export function countByCategory(items: readonly InboxItem[]): InboxCounts {
  const counts: InboxCounts = {
    total: items.length,
    review: 0,
    blocked: 0,
    permission: 0,
    decision: 0,
    leaderDenied: 0,
  };
  for (const item of items) {
    switch (item.category) {
      case "review":
        counts.review += 1;
        break;
      case "blocked":
        counts.blocked += 1;
        break;
      case "permission":
        counts.permission += 1;
        break;
      case "decision":
        counts.decision += 1;
        break;
      case "leader_denied":
        counts.leaderDenied += 1;
        break;
    }
  }
  return counts;
}

/** 待我处理的总数：后端给了就用后端的，没有就本地数。 */
export function inboxTotal(counts: InboxCounts | undefined, items: readonly InboxItem[]): number {
  return counts?.total ?? items.length;
}

// ---------------------------------------------------------------- 跳转目标

/** 任务详情地址。 */
export function taskPath(teamId: string, taskId: string): string {
  return `/teams/${teamId}/tasks/${taskId}`;
}

/** 验收页地址。 */
export function reviewPath(teamId: string, taskId: string): string {
  return `/teams/${teamId}/tasks/${taskId}/review`;
}

/** 岗位通信页地址（带线程，M5-03 的页面按 `?thread=` 打开指定线程）。 */
export function threadPath(teamId: string, threadId: string): string {
  return `/teams/${teamId}/messages?thread=${threadId}`;
}

/** 活动页地址。 */
export function activityPath(teamId: string): string {
  return `/teams/${teamId}/activity`;
}

/** 待我处理条目的跳转地址。 */
export function inboxTargetPath(teamId: string, item: InboxItem): string {
  const target: InboxTarget = item.target;
  switch (target.type) {
    case "task":
      // 待验收的事项直接落到验收页，其余落到任务详情。
      return item.category === "review"
        ? reviewPath(teamId, target.taskId)
        : taskPath(teamId, target.taskId);
    case "message":
      return threadPath(teamId, target.threadId);
    case "permission":
      // 权限弹窗在任务详情的执行区里，跳任务详情即可（弹窗会自动打开）。
      return item.taskId === null ? activityPath(teamId) : taskPath(teamId, item.taskId);
    case "activity":
      return activityPath(teamId);
  }
}

/**
 * 条目的底色：需要你拍板或处理异常的两类用琥珀底色显眼标出，
 * 其余保持中性白底（与 Figma「待我处理」画板一致）。
 */
export function inboxItemTone(category: InboxCategory): "plain" | "warn" {
  return category === "blocked" || category === "decision" ? "warn" : "plain";
}

/** 待我处理条目的操作按钮文案。 */
export function inboxActionLabel(item: InboxItem): string {
  switch (item.category) {
    case "review":
      return "去验收";
    case "blocked":
      return "查看任务详情";
    case "permission":
      return "去答复权限";
    case "decision":
      return "查看消息线程";
    case "leader_denied":
      return "查看活动";
  }
}

/** 通知的跳转地址。用通知自带的 `teamId`：通知是按团队产生的。 */
export function notificationTargetPath(teamId: string, notification: Notification): string {
  if (notification.taskId !== null) {
    return taskPath(teamId, notification.taskId);
  }
  if (notification.threadId !== null) {
    return threadPath(teamId, notification.threadId);
  }
  return activityPath(teamId);
}

// ---------------------------------------------------------------- 通知中心

/** 会话内收到的一条通知（带已读标记）。通知不落库，只在页面打开期间攒着。 */
export interface SessionNotification {
  notification: Notification;
  read: boolean;
}

/**
 * 只留当前团队的通知：通知按团队产生，通知中心只看当前团队，不串到别的团队去。
 */
export function notificationsForTeam(
  entries: readonly SessionNotification[],
  teamId: string,
): Notification[] {
  return entries
    .filter((entry) => entry.notification.teamId === teamId)
    .map((entry) => entry.notification);
}

/** 当前团队的未读数：别的团队的未读不计入。 */
export function unreadForTeam(entries: readonly SessionNotification[], teamId: string): number {
  return entries.filter((entry) => !entry.read && entry.notification.teamId === teamId).length;
}

// ---------------------------------------------------------------- 记录决策

/** 决策内容上限，与后端 `MAX_DECISION_TEXT` 一致。 */
export const MAX_DECISION_TEXT = 2000;

/**
 * 决策内容校验：与后端 `service::text` 同规则（必填、长度上限）。
 * 返回错误文案；可以提交时返回 null。
 */
export function validateDecisionText(text: string): string | null {
  const value = text.trim();
  if (value.length === 0) {
    return "决策内容不能为空";
  }
  if (Array.from(value).length > MAX_DECISION_TEXT) {
    return `决策内容不能超过 ${String(MAX_DECISION_TEXT)} 个字符`;
  }
  return null;
}

/** 决策弹窗是否可提交。 */
export function canSubmitDecision(text: string, submitting: boolean): boolean {
  return !submitting && validateDecisionText(text) === null;
}

/** 决策提交按钮的文案。 */
export function decisionSubmitLabel(resumeTask: boolean): string {
  return resumeTask ? "保存决策并恢复任务" : "保存决策";
}

/**
 * 能否勾选「同时把任务恢复到 READY」：
 * 只有关联任务当前是「受阻」时才可以（后端会做同样的校验）。
 */
export function canResumeTask(taskStatus: string | undefined): boolean {
  return taskStatus === "blocked";
}

// ---------------------------------------------------------------- 通知订阅

/** 通知订阅的草稿：类别总开关 + 每个类别的岗位开关。 */
export interface SubscriptionDraft {
  /** 类别总开关。 */
  categoryEnabled: Record<string, boolean>;
  /** 岗位开关：类别 → 岗位 id → 开关。 */
  roleEnabled: Record<string, Record<string, boolean>>;
}

/** 由后端设置生成草稿。 */
export function draftFromSettings(settings: NotificationSettings): SubscriptionDraft {
  const categoryEnabled: Record<string, boolean> = {};
  const roleEnabled: Record<string, Record<string, boolean>> = {};
  for (const category of settings.categories) {
    categoryEnabled[category.category] = category.enabled;
    const roles: Record<string, boolean> = {};
    for (const role of category.roles) {
      roles[role.roleId] = role.enabled;
    }
    roleEnabled[category.category] = roles;
  }
  return { categoryEnabled, roleEnabled };
}

/** 改一个类别的总开关。岗位开关原样保留（类别关掉时岗位一律不生效）。 */
export function setCategoryEnabled(
  draft: SubscriptionDraft,
  category: NotificationCategory,
  enabled: boolean,
): SubscriptionDraft {
  return {
    ...draft,
    categoryEnabled: { ...draft.categoryEnabled, [category]: enabled },
  };
}

/** 改一个类别下的岗位开关。类别总开关关着时改不动。 */
export function setRoleEnabled(
  draft: SubscriptionDraft,
  category: NotificationCategory,
  roleId: string,
  enabled: boolean,
): SubscriptionDraft {
  if (draft.categoryEnabled[category] !== true) {
    return draft;
  }
  const roles = { ...(draft.roleEnabled[category] ?? {}) };
  roles[roleId] = enabled;
  return {
    ...draft,
    roleEnabled: { ...draft.roleEnabled, [category]: roles },
  };
}

/** 岗位开关的实际生效值：类别总开关关掉时，岗位一律不生效。 */
export function isRoleEnabled(
  draft: SubscriptionDraft,
  category: NotificationCategory,
  roleId: string,
): boolean {
  if (draft.categoryEnabled[category] !== true) {
    return false;
  }
  return draft.roleEnabled[category]?.[roleId] === true;
}

/**
 * 比较草稿与后端设置，算出要提交的改动。
 * 没有改动的类别不提交（后端「未列出的类别保持不变」）。
 */
export function diffDraft(
  settings: NotificationSettings,
  draft: SubscriptionDraft,
): NotificationCategoryUpdate[] {
  const updates: NotificationCategoryUpdate[] = [];
  for (const category of settings.categories) {
    const masterChanged = draft.categoryEnabled[category.category] !== category.enabled;
    const roles: { roleId: string; enabled: boolean }[] = [];
    for (const role of category.roles) {
      const next = draft.roleEnabled[category.category]?.[role.roleId] === true;
      if (next !== role.enabled) {
        roles.push({ roleId: role.roleId, enabled: next });
      }
    }
    if (!masterChanged && roles.length === 0) {
      continue;
    }
    updates.push({
      category: category.category,
      enabled: draft.categoryEnabled[category.category] === true,
      roles,
    });
  }
  return updates;
}

/** 草稿是否有改动。 */
export function draftChanged(settings: NotificationSettings, draft: SubscriptionDraft): boolean {
  return diffDraft(settings, draft).length > 0;
}

/** 通知类别的兜底名（后端给了 `categoryLabel` 时用后端的）。 */
export const NOTIFICATION_CATEGORY_FALLBACK_LABEL: Record<NotificationCategory, string> = {
  task_review: "任务待验收",
  task_failed: "运行失败",
  task_blocked: "任务受阻",
  message_replied: "消息已回复",
  message_action: "消息需要处理",
  permission: "权限请求",
};

/** 通知类别的严重级别（界面上的颜色依据，与后端投递策略一致）。 */
export const NOTIFICATION_CATEGORY_SEVERITY: Record<
  NotificationCategory,
  "normal" | "important" | "high"
> = {
  task_review: "normal",
  task_failed: "important",
  task_blocked: "important",
  message_replied: "normal",
  message_action: "normal",
  permission: "high",
};

/** 通知类别的默认接收岗位（界面说明用；实际开关由后端设置给出）。 */
export const NOTIFICATION_CATEGORY_DEFAULT_ROLES: Record<NotificationCategory, string> = {
  task_review: "任务负责岗位 · Leader",
  task_failed: "任务负责岗位 · Leader",
  task_blocked: "任务负责岗位 · Leader",
  message_replied: "发起岗位 · Leader",
  message_action: "发起岗位 · Leader",
  permission: "Human · Leader",
};

/** 按类别取设置的显示名。 */
export function categoryLabel(category: NotificationCategorySetting): string {
  return category.categoryLabel || NOTIFICATION_CATEGORY_FALLBACK_LABEL[category.category];
}
