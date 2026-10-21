// 岗位通信页的纯逻辑：状态名与颜色、发送方名、筛选参数、时间格式与轮询开合。
// 界面上的文字都来自这些纯函数，单元测试锁住文案与判定，组件只负责排版。
import { describe, expect, it, vi } from "vitest";

import type { Message } from "../src/bindings/Message.js";
import type { MessageStatus } from "../src/bindings/MessageStatus.js";
import type { MessageThread } from "../src/bindings/MessageThread.js";
import type { ThreadParticipant } from "../src/bindings/ThreadParticipant.js";
import {
  MESSAGE_STATUS_LABELS,
  MESSAGE_STATUS_TONES,
  ROLE_PARAM,
  TASK_PARAM,
  canRetry,
  formatMessageTime,
  isPendingStatus,
  latestMessage,
  participantNames,
  participantsLabel,
  parseThreadFilters,
  senderName,
  serializeThreadFilters,
  shouldPollMessages,
  shouldPollThreads,
  threadSnippet,
} from "../src/pages/messages/messagesUi.js";
import {
  createMessagePoller,
  MESSAGE_POLL_INTERVAL_MS,
} from "../src/pages/messages/useMessagePolling.js";

/** 只用到界面关心字段的最小消息。 */
function messageWith(status: MessageStatus, extra: Partial<Message> = {}): Message {
  return {
    id: "msg-1",
    threadId: "thread-1",
    teamId: "team-1",
    taskId: null,
    sender: "human",
    senderRoleId: null,
    toRoleId: "role-qa",
    body: "请阅读登录验收规范并完成回归。",
    status,
    failureReason: null,
    replyBody: null,
    createdAt: "2026-10-21T02:31:00Z",
    deliveredAt: null,
    repliedAt: null,
    ...extra,
  };
}

/** 只用到界面关心字段的最小线程（列表项直接带最近一条消息的摘要）。 */
function threadWith(extra: Partial<MessageThread> = {}): MessageThread {
  return {
    id: "thread-1",
    teamId: "team-1",
    taskId: null,
    subject: "登录验收",
    participants: [],
    createdAt: "2026-10-21T02:30:00Z",
    updatedAt: "2026-10-21T02:31:00Z",
    lastStatus: "queued",
    lastMessageAt: "2026-10-21T02:31:00Z",
    lastSnippet: "请阅读登录验收规范并完成回归。",
    ...extra,
  };
}

const ROLE_NAMES = new Map([
  ["role-qa", "QA 测试"],
  ["role-fe", "前端开发"],
]);

describe("消息状态", () => {
  it("状态名与后端一致", () => {
    expect(MESSAGE_STATUS_LABELS).toEqual({
      queued: "排队中",
      delivering: "投递中",
      replied: "已回复",
      failed: "失败",
      expired: "已过期",
    });
  });

  it("每个状态都有颜色，不靠颜色单独表达状态", () => {
    for (const status of Object.keys(MESSAGE_STATUS_LABELS) as MessageStatus[]) {
      expect(MESSAGE_STATUS_TONES[status]).toBeDefined();
    }
    expect(MESSAGE_STATUS_TONES.replied).toBe("success");
    expect(MESSAGE_STATUS_TONES.failed).toBe("danger");
  });

  it("排队中与投递中是进行中，其余是终态", () => {
    expect(isPendingStatus("queued")).toBe(true);
    expect(isPendingStatus("delivering")).toBe(true);
    expect(isPendingStatus("replied")).toBe(false);
    expect(isPendingStatus("failed")).toBe(false);
    expect(isPendingStatus("expired")).toBe(false);
  });

  it("只有失败与过期可以重新发送", () => {
    expect(canRetry("failed")).toBe(true);
    expect(canRetry("expired")).toBe(true);
    expect(canRetry("queued")).toBe(false);
    expect(canRetry("delivering")).toBe(false);
    expect(canRetry("replied")).toBe(false);
  });
});

describe("轮询开合", () => {
  it("有排队中或投递中的消息时才继续刷新", () => {
    expect(shouldPollMessages([messageWith("queued")])).toBe(true);
    expect(shouldPollMessages([messageWith("delivering")])).toBe(true);
    expect(shouldPollMessages([messageWith("replied")])).toBe(false);
    expect(shouldPollMessages([messageWith("failed")])).toBe(false);
    expect(shouldPollMessages([messageWith("replied"), messageWith("delivering")])).toBe(true);
    expect(shouldPollMessages([])).toBe(false);
  });

  it("线程列表里有排队中或投递中的线程时继续刷新", () => {
    expect(shouldPollThreads([threadWith({ lastStatus: "queued" })])).toBe(true);
    expect(shouldPollThreads([threadWith({ lastStatus: "delivering" })])).toBe(true);
    expect(shouldPollThreads([threadWith({ lastStatus: "replied" })])).toBe(false);
    expect(shouldPollThreads([threadWith({ lastStatus: "failed" })])).toBe(false);
    expect(shouldPollThreads([])).toBe(false);
  });

  it("轮询间隔与执行详情一致，重复启动不会叠加定时器", () => {
    expect(MESSAGE_POLL_INTERVAL_MS).toBe(3000);
    vi.useFakeTimers();
    const tick = vi.fn();
    const poller = createMessagePoller(tick, 10);
    poller.start();
    poller.start();
    vi.advanceTimersByTime(30);
    expect(tick).toHaveBeenCalledTimes(3);
    poller.stop();
    vi.advanceTimersByTime(50);
    expect(tick).toHaveBeenCalledTimes(3);
    vi.useRealTimers();
  });
});

describe("发送方与参与方", () => {
  it("人发的消息显示 Human，Leader 发的显示 Leader", () => {
    expect(senderName(messageWith("queued"), ROLE_NAMES)).toBe("Human");
    expect(senderName(messageWith("queued", { sender: "leader" }), ROLE_NAMES)).toBe("Leader");
  });

  it("岗位发的消息显示岗位名", () => {
    expect(senderName(messageWith("queued", { sender: "role" }), ROLE_NAMES)).toBe("QA 测试");
  });

  it("岗位名查不到时退回「岗位」", () => {
    expect(senderName(messageWith("queued", { sender: "role" }), new Map())).toBe("岗位");
  });

  it("参与方去重后按顺序显示", () => {
    const participants: ThreadParticipant[] = [
      { roleId: null, kind: "human", name: "Human" },
      { roleId: "role-qa", kind: "role", name: "QA 测试" },
      { roleId: "role-qa", kind: "role", name: "QA 测试" },
    ];
    expect(participantNames(participants)).toEqual(["Human", "QA 测试"]);
    expect(participantsLabel(participants)).toBe("Human ↔ QA 测试");
  });
});

describe("筛选参数", () => {
  it("空值表示不过滤", () => {
    expect(parseThreadFilters(new URLSearchParams())).toEqual({ roleId: "", taskId: "" });
  });

  it("读出地址栏里的岗位与任务筛选", () => {
    const params = new URLSearchParams(`role=role-qa&${TASK_PARAM}=task-1`);
    expect(parseThreadFilters(params)).toEqual({ roleId: "role-qa", taskId: "task-1" });
  });

  it("写回时不给默认值留位置，链接保持干净", () => {
    const params = serializeThreadFilters({ roleId: "", taskId: "" });
    expect(params.toString()).toBe("");
    const filled = serializeThreadFilters({ roleId: "role-qa", taskId: "task-1" });
    expect(filled.get(ROLE_PARAM)).toBe("role-qa");
    expect(filled.get(TASK_PARAM)).toBe("task-1");
  });
});

describe("时间与摘要", () => {
  const now = new Date("2026-10-21T12:00:00+08:00");

  it("今天的消息记「今天 时:分」", () => {
    expect(formatMessageTime("2026-10-21T02:31:00Z", now)).toBe("今天 10:31");
  });

  it("昨天的消息记「昨天 时:分」", () => {
    expect(formatMessageTime("2026-10-20T02:31:00Z", now)).toBe("昨天 10:31");
  });

  it("更早的消息记「月/日 时:分」", () => {
    expect(formatMessageTime("2026-10-18T02:31:00Z", now)).toBe("10/18 10:31");
  });

  it("时间无法解析时原样返回", () => {
    expect(formatMessageTime("不是时间", now)).toBe("不是时间");
  });

  it("摘要用后端给的最近一条消息摘要", () => {
    expect(threadSnippet(threadWith())).toBe("请阅读登录验收规范并完成回归。");
    expect(threadSnippet(threadWith({ lastSnippet: "第二条 换行" }))).toBe("第二条 换行");
  });

  it("没有摘要时为空", () => {
    expect(threadSnippet(threadWith({ lastSnippet: null }))).toBe("");
    expect(latestMessage([])).toBeNull();
  });
});
