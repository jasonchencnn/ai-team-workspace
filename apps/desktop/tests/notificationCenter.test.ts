// 通知中心纯逻辑：只显示当前团队的通知、未读数按当前团队计、跳转路径用通知自带的团队。
import { describe, expect, it } from "vitest";

import type { Notification } from "../src/bindings/Notification.js";
import {
  activityPath,
  notificationsForTeam,
  notificationTargetPath,
  taskPath,
  threadPath,
  unreadForTeam,
  type SessionNotification,
} from "../src/pages/inbox/inboxUi.js";

/** 造一条通知。 */
function notification(
  partial: Partial<Notification> & Pick<Notification, "id" | "teamId">,
): Notification {
  return {
    id: partial.id,
    teamId: partial.teamId,
    category: partial.category ?? "task_review",
    categoryLabel: partial.categoryLabel ?? "任务待验收",
    title: partial.title ?? "通知标题",
    body: partial.body ?? "",
    taskId: partial.taskId ?? null,
    taskKey: partial.taskKey ?? null,
    messageId: partial.messageId ?? null,
    threadId: partial.threadId ?? null,
    runId: partial.runId ?? null,
    roleId: partial.roleId ?? null,
    occurredAt: partial.occurredAt ?? "2026-10-21T00:00:00Z",
  };
}

/** 造一条会话内通知（默认未读）。 */
function entry(note: Notification, read = false): SessionNotification {
  return { notification: note, read };
}

const ENTRIES: SessionNotification[] = [
  entry(notification({ id: "n-1", teamId: "team-1", taskId: "t-1" })),
  entry(notification({ id: "n-2", teamId: "team-2", taskId: "t-2" })),
  entry(notification({ id: "n-3", teamId: "team-1", threadId: "th-3" }), true),
];

describe("通知中心按团队过滤", () => {
  it("只显示当前团队的通知", () => {
    const shown = notificationsForTeam(ENTRIES, "team-1");

    expect(shown.map((note) => note.id)).toEqual(["n-1", "n-3"]);
  });

  it("别的团队的通知不会串过来", () => {
    const shown = notificationsForTeam(ENTRIES, "team-2");

    expect(shown.map((note) => note.id)).toEqual(["n-2"]);
  });

  it("没有通知的团队看到的是空列表", () => {
    expect(notificationsForTeam(ENTRIES, "team-3")).toEqual([]);
  });
});

describe("通知中心未读数", () => {
  it("只数当前团队的未读", () => {
    expect(unreadForTeam(ENTRIES, "team-1")).toBe(1);
    expect(unreadForTeam(ENTRIES, "team-2")).toBe(1);
  });

  it("读了之后未读数归零", () => {
    const read = ENTRIES.map((item) => ({ ...item, read: true }));

    expect(unreadForTeam(read, "team-1")).toBe(0);
    expect(unreadForTeam(read, "team-2")).toBe(0);
  });
});

describe("通知跳转路径", () => {
  it("用通知自带的团队 id 拼路径", () => {
    const withTask = notification({ id: "n-1", teamId: "team-2", taskId: "t-1" });
    const withThread = notification({ id: "n-2", teamId: "team-2", threadId: "th-1" });
    const bare = notification({ id: "n-3", teamId: "team-2" });

    expect(notificationTargetPath(withTask.teamId, withTask)).toBe(taskPath("team-2", "t-1"));
    expect(notificationTargetPath(withThread.teamId, withThread)).toBe(
      threadPath("team-2", "th-1"),
    );
    expect(notificationTargetPath(bare.teamId, bare)).toBe(activityPath("team-2"));
  });
});
