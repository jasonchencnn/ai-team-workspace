// 消息气泡的渲染：发送方、正文、状态标签、回复正文与时间；失败与过期显示原因与「重新发送」。
// 用 react-dom/server 把组件渲染成 HTML 再断言，页面上的排版细节由端到端测试覆盖。
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import type { Message } from "../src/bindings/Message.js";
import type { MessageStatus } from "../src/bindings/MessageStatus.js";
import { MessageBubble } from "../src/pages/messages/MessageBubble.js";

const ROLE_NAMES = new Map([
  ["role-qa", "QA 测试"],
  ["role-fe", "前端开发"],
]);

/**
 * 固定「现在」与两条消息时间。
 *
 * 时间按本地时区构造（不是写死的 UTC 字符串），这样两条消息与「现在」永远落在同一个
 * 本地日里，断言不受运行日期与所在时区影响——之前写死 2026-10-21 的 UTC 时间，
 * 过了当天零点就变成「昨天」，测试会自己挂掉。
 */
const NOW = new Date(2026, 9, 21, 12, 0, 0);
const CREATED_AT = new Date(2026, 9, 21, 10, 31, 0).toISOString();
const REPLIED_AT = new Date(2026, 9, 21, 11, 2, 0).toISOString();

/** 只用到界面关心字段的最小消息。 */
function messageWith(status: MessageStatus, extra: Partial<Message> = {}): Message {
  return {
    id: "msg-1",
    threadId: "thread-1",
    teamId: "team-1",
    taskId: "task-1",
    sender: "human",
    senderRoleId: null,
    toRoleId: "role-qa",
    body: "请阅读登录验收规范并完成回归。",
    status,
    failureReason: null,
    replyBody: null,
    createdAt: CREATED_AT,
    deliveredAt: null,
    repliedAt: null,
    bindingHarnessId: null,
    ...extra,
  };
}

function render(props: Parameters<typeof MessageBubble>[0]): string {
  return renderToStaticMarkup(createElement(MessageBubble, { now: NOW, ...props }));
}

describe("消息气泡", () => {
  it("显示发送方、正文与状态标签", () => {
    const html = render({ message: messageWith("queued"), roleNames: ROLE_NAMES });

    expect(html).toContain("Human");
    expect(html).toContain("请阅读登录验收规范并完成回归。");
    expect(html).toContain("排队中");
    expect(html).toContain("今天 10:31");
  });

  it("投递中与已回复各有自己的状态标签", () => {
    expect(render({ message: messageWith("delivering"), roleNames: ROLE_NAMES })).toContain(
      "投递中",
    );
    expect(render({ message: messageWith("replied"), roleNames: ROLE_NAMES })).toContain("已回复");
  });

  it("已回复的消息显示回复正文与回复时间", () => {
    const html = render({
      message: messageWith("replied", {
        replyBody: "已确认，将按引用规范执行。",
        repliedAt: REPLIED_AT,
      }),
      roleNames: ROLE_NAMES,
    });

    expect(html).toContain("已确认，将按引用规范执行。");
    expect(html).toContain("今天 11:02");
    // 回复来自接收岗位，气泡里显示它的名字。
    expect(html).toContain("QA 测试 / 已回复");
  });

  it("失败的消息显示原因与重新发送", () => {
    const onRetry = vi.fn();
    const html = render({
      message: messageWith("failed", { failureReason: "Agent 出错：Mock 模拟的回复失败" }),
      roleNames: ROLE_NAMES,
      onRetry,
    });

    expect(html).toContain("失败原因：Agent 出错：Mock 模拟的回复失败");
    expect(html).toContain("重新发送");
  });

  it("过期的消息也能重新发送", () => {
    const html = render({
      message: messageWith("expired"),
      roleNames: ROLE_NAMES,
      onRetry: vi.fn(),
    });

    expect(html).toContain("已过期");
    expect(html).toContain("重新发送");
  });

  it("排队中与投递中的消息没有重新发送入口", () => {
    const html = render({
      message: messageWith("delivering"),
      roleNames: ROLE_NAMES,
      onRetry: vi.fn(),
    });

    expect(html).not.toContain("重新发送");
  });

  it("岗位发出的消息显示岗位名", () => {
    const html = render({
      message: messageWith("replied", { sender: "role" }),
      roleNames: ROLE_NAMES,
    });

    expect(html).toContain("QA 测试");
  });
});
