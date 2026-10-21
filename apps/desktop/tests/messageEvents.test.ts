// 消息事件的判定：内核的通知通道里既有通知也有消息状态变化，
// 岗位通信页只对「本团队的消息状态变化」作出反应。
import { describe, expect, it } from "vitest";

import type { Message } from "../src/bindings/Message.js";
import type { Notification } from "../src/bindings/Notification.js";
import type { NotifyEvent } from "../src/bindings/NotifyEvent.js";
import { messageStatusChange } from "../src/pages/messages/messageEvents.js";

/** 只用到界面关心字段的最小消息。 */
function messageWith(teamId: string, threadId: string): Message {
  return {
    id: "msg-1",
    threadId,
    teamId,
    taskId: null,
    sender: "human",
    senderRoleId: null,
    toRoleId: "role-qa",
    body: "请阅读登录验收规范并完成回归。",
    status: "replied",
    failureReason: null,
    replyBody: "已收到",
    createdAt: "2026-10-21T02:31:00Z",
    deliveredAt: null,
    repliedAt: "2026-10-21T03:02:00Z",
    bindingHarnessId: null,
  };
}

const NOTIFICATION: Notification = {
  id: "note-1",
  teamId: "team-1",
  category: "message_action",
  categoryLabel: "消息需要处理",
  title: "岗位已回复消息",
  body: "QA 测试回复了一条消息。",
  taskId: null,
  taskKey: null,
  messageId: "msg-1",
  threadId: "thread-1",
  runId: null,
  roleId: "role-qa",
  occurredAt: "2026-10-21T03:02:00Z",
};

describe("消息状态事件", () => {
  it("本团队的消息状态变化给出线程 id", () => {
    const event: NotifyEvent = {
      type: "message_status",
      message: messageWith("team-1", "thread-1"),
      previousStatus: "delivering",
    };

    expect(messageStatusChange(event, "team-1")).toEqual({
      threadId: "thread-1",
      teamId: "team-1",
    });
  });

  it("通知类事件与岗位通信页无关", () => {
    const event: NotifyEvent = { type: "notification", notification: NOTIFICATION };

    expect(messageStatusChange(event, "team-1")).toBeNull();
  });

  it("别的团队的消息不处理", () => {
    const event: NotifyEvent = {
      type: "message_status",
      message: messageWith("team-2", "thread-9"),
      previousStatus: null,
    };

    expect(messageStatusChange(event, "team-1")).toBeNull();
  });
});
