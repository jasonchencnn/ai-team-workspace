// 待我处理页纯逻辑：分组与计数、决策弹窗校验、通知订阅开关、跳转目标。
import { describe, expect, it } from "vitest";

import type { InboxCounts } from "../src/bindings/InboxCounts.js";
import type { InboxItem } from "../src/bindings/InboxItem.js";
import type { Notification } from "../src/bindings/Notification.js";
import type { NotificationSettings } from "../src/bindings/NotificationSettings.js";
import {
  activityPath,
  canResumeTask,
  canSubmitDecision,
  countByCategory,
  decisionSubmitLabel,
  diffDraft,
  draftChanged,
  draftFromSettings,
  groupInboxItems,
  inboxActionLabel,
  inboxItemTone,
  inboxTargetPath,
  inboxTotal,
  isRoleEnabled,
  notificationTargetPath,
  reviewPath,
  roleSwitchValue,
  setCategoryEnabled,
  setRoleEnabled,
  taskPath,
  threadPath,
  validateDecisionText,
} from "../src/pages/inbox/inboxUi.js";

/** 造一条待我处理事项。 */
function item(partial: Partial<InboxItem> & Pick<InboxItem, "id" | "category">): InboxItem {
  return {
    id: partial.id,
    category: partial.category,
    categoryLabel: partial.categoryLabel ?? "待验收",
    title: partial.title ?? "标题",
    taskId: partial.taskId ?? null,
    taskKey: partial.taskKey ?? null,
    messageId: partial.messageId ?? null,
    threadId: partial.threadId ?? null,
    runId: partial.runId ?? null,
    occurredAt: partial.occurredAt ?? "2026-10-21T00:00:00Z",
    impact: partial.impact ?? "影响说明",
    suggestion: partial.suggestion ?? "建议操作",
    target: partial.target ?? { type: "activity", eventId: "evt-1" },
    noChanges: partial.noChanges ?? false,
  };
}

const ITEMS: InboxItem[] = [
  item({
    id: "review:1",
    category: "review",
    categoryLabel: "待验收",
    taskId: "t-1",
    taskKey: "LD-008",
    target: { type: "task", taskId: "t-1", taskKey: "LD-008" },
  }),
  item({
    id: "blocked:2",
    category: "blocked",
    categoryLabel: "执行失败 / 受阻",
    taskId: "t-2",
    taskKey: "FE-023",
    target: { type: "task", taskId: "t-2", taskKey: "FE-023" },
  }),
  item({ id: "blocked:3", category: "blocked", categoryLabel: "执行失败 / 受阻", taskId: "t-3" }),
  item({ id: "permission:4", category: "permission", categoryLabel: "待答复权限", taskId: "t-4" }),
  item({ id: "decision:5", category: "decision", categoryLabel: "需要拍板的消息" }),
  item({ id: "leader:6", category: "leader_denied", categoryLabel: "Leader 越权" }),
];

/** 取固定位置的事项（缺了就报错，不用非空断言）。 */
function at(index: number): InboxItem {
  const found = ITEMS[index];
  if (found === undefined) {
    throw new Error(`测试数据里没有第 ${String(index)} 条事项`);
  }
  return found;
}

describe("groupInboxItems", () => {
  it("按固定类别顺序分组，组内保持后端顺序", () => {
    const groups = groupInboxItems(ITEMS);

    expect(groups.map((group) => group.category)).toEqual([
      "review",
      "blocked",
      "permission",
      "decision",
      "leader_denied",
    ]);
    expect(groups[1]?.items.map((entry) => entry.id)).toEqual(["blocked:2", "blocked:3"]);
  });

  it("空类别不出现，类别名用后端给的", () => {
    const groups = groupInboxItems([at(0)]);

    expect(groups).toHaveLength(1);
    expect(groups[0]?.label).toBe("待验收");
  });

  it("没有事项时分组为空", () => {
    expect(groupInboxItems([])).toEqual([]);
  });
});

describe("countByCategory", () => {
  it("按类别计数并给出总数", () => {
    expect(countByCategory(ITEMS)).toEqual({
      total: 6,
      review: 1,
      blocked: 2,
      permission: 1,
      decision: 1,
      leaderDenied: 1,
    });
  });

  it("空列表的计数全为 0", () => {
    expect(countByCategory([])).toEqual({
      total: 0,
      review: 0,
      blocked: 0,
      permission: 0,
      decision: 0,
      leaderDenied: 0,
    });
  });
});

describe("inboxTotal", () => {
  const counts: InboxCounts = {
    total: 6,
    review: 1,
    blocked: 2,
    permission: 1,
    decision: 1,
    leaderDenied: 1,
  };

  it("后端给了计数就用后端的", () => {
    expect(inboxTotal(counts, [])).toBe(6);
  });

  it("没有后端计数时本地数", () => {
    expect(inboxTotal(undefined, ITEMS)).toBe(6);
  });
});

describe("validateDecisionText", () => {
  it("空内容不通过", () => {
    expect(validateDecisionText("")).toBe("决策内容不能为空");
    expect(validateDecisionText("   \n ")).toBe("决策内容不能为空");
  });

  it("超过上限不通过", () => {
    expect(validateDecisionText("a".repeat(2001))).toContain("2000");
  });

  it("正常内容通过", () => {
    expect(validateDecisionText("  保持登录有效期：7 天  ")).toBeNull();
  });
});

describe("canSubmitDecision", () => {
  it("内容为空或正在提交时不可提交", () => {
    expect(canSubmitDecision("", false)).toBe(false);
    expect(canSubmitDecision("决策", true)).toBe(false);
  });

  it("内容有效且没有提交中时可提交", () => {
    expect(canSubmitDecision("决策", false)).toBe(true);
  });
});

describe("decisionSubmitLabel", () => {
  it("勾选恢复任务时按钮文案带上恢复", () => {
    expect(decisionSubmitLabel(true)).toBe("保存决策并恢复任务");
    expect(decisionSubmitLabel(false)).toBe("保存决策");
  });
});

describe("canResumeTask", () => {
  it("只有受阻任务可以恢复", () => {
    expect(canResumeTask("blocked")).toBe(true);
    expect(canResumeTask("doing")).toBe(false);
    expect(canResumeTask("ready")).toBe(false);
    expect(canResumeTask(undefined)).toBe(false);
  });
});

const SETTINGS: NotificationSettings = {
  teamId: "team-1",
  categories: [
    {
      category: "task_review",
      categoryLabel: "任务待验收",
      enabled: true,
      roles: [
        { roleId: "r-1", roleName: "前端开发", roleCode: "role_frontend", enabled: true },
        { roleId: "r-2", roleName: "Leader", roleCode: "role_leader", enabled: false },
      ],
    },
    {
      category: "permission",
      categoryLabel: "权限请求",
      enabled: true,
      roles: [
        { roleId: "r-1", roleName: "前端开发", roleCode: "role_frontend", enabled: true },
        { roleId: "r-2", roleName: "Leader", roleCode: "role_leader", enabled: true },
      ],
    },
  ],
};

describe("通知订阅开关", () => {
  it("草稿按后端设置初始化", () => {
    const draft = draftFromSettings(SETTINGS);

    expect(draft.categoryEnabled.task_review).toBe(true);
    expect(isRoleEnabled(draft, "task_review", "r-1")).toBe(true);
    expect(isRoleEnabled(draft, "task_review", "r-2")).toBe(false);
  });

  it("关掉类别总开关后岗位一律不生效", () => {
    const draft = setCategoryEnabled(draftFromSettings(SETTINGS), "task_review", false);

    expect(isRoleEnabled(draft, "task_review", "r-1")).toBe(false);
    expect(isRoleEnabled(draft, "task_review", "r-2")).toBe(false);
  });

  it("类别总开关关着时岗位开关改不动", () => {
    const off = setCategoryEnabled(draftFromSettings(SETTINGS), "task_review", false);
    const after = setRoleEnabled(off, "task_review", "r-2", true);

    expect(after.roleEnabled.task_review?.["r-2"]).toBe(false);
  });

  it("类别总开关开着时岗位开关可以单独关", () => {
    const draft = setRoleEnabled(draftFromSettings(SETTINGS), "task_review", "r-1", false);

    expect(isRoleEnabled(draft, "task_review", "r-1")).toBe(false);
    expect(isRoleEnabled(draft, "permission", "r-1")).toBe(true);
  });

  it("只提交有改动的类别，并带上岗位改动", () => {
    const draft = setRoleEnabled(draftFromSettings(SETTINGS), "task_review", "r-2", true);
    const updates = diffDraft(SETTINGS, draft);

    expect(updates).toHaveLength(1);
    expect(updates[0]?.category).toBe("task_review");
    expect(updates[0]?.enabled).toBe(true);
    expect(updates[0]?.roles).toEqual([{ roleId: "r-2", enabled: true }]);
  });

  it("关掉类别总开关时提交 enabled=false", () => {
    const draft = setCategoryEnabled(draftFromSettings(SETTINGS), "permission", false);
    const updates = diffDraft(SETTINGS, draft);

    expect(updates).toEqual([{ category: "permission", enabled: false, roles: [] }]);
  });

  it("没有改动时不提交", () => {
    const draft = draftFromSettings(SETTINGS);

    expect(diffDraft(SETTINGS, draft)).toEqual([]);
    expect(draftChanged(SETTINGS, draft)).toBe(false);
    expect(draftChanged(SETTINGS, setCategoryEnabled(draft, "permission", false))).toBe(true);
  });

  it("总开关关着时岗位开关保留自己的值", () => {
    const off = setCategoryEnabled(draftFromSettings(SETTINGS), "task_review", false);

    // 前端岗位本来就是开的，Leader 本来就是关的：总开关关着也要看得出来。
    expect(roleSwitchValue(off, "task_review", "r-1")).toBe(true);
    expect(roleSwitchValue(off, "task_review", "r-2")).toBe(false);
    // 但实际生效值仍由总开关决定。
    expect(isRoleEnabled(off, "task_review", "r-1")).toBe(false);
  });

  it("总开关关着时不提交未改动的岗位项", () => {
    const off = setCategoryEnabled(draftFromSettings(SETTINGS), "task_review", false);
    const updates = diffDraft(SETTINGS, off);

    // 岗位开关原样保留，提交里不该带上任何岗位。
    expect(updates).toEqual([{ category: "task_review", enabled: false, roles: [] }]);
  });

  it("重新打开总开关时岗位开关还是原来的值", () => {
    const off = setCategoryEnabled(draftFromSettings(SETTINGS), "task_review", false);
    const on = setCategoryEnabled(off, "task_review", true);

    expect(roleSwitchValue(on, "task_review", "r-1")).toBe(true);
    expect(roleSwitchValue(on, "task_review", "r-2")).toBe(false);
    // 岗位设置没被动过，总开关也回到了原样：没有要提交的东西。
    expect(diffDraft(SETTINGS, on)).toEqual([]);
  });
});

describe("跳转目标", () => {
  it("待验收落到验收页，其余任务落到任务详情", () => {
    const review = at(0);
    const blocked = at(1);

    expect(inboxTargetPath("team-1", review)).toBe(reviewPath("team-1", "t-1"));
    expect(inboxTargetPath("team-1", blocked)).toBe(taskPath("team-1", "t-2"));
  });

  it("消息落到线程页，越权落到活动页", () => {
    const message = item({
      id: "message:1",
      category: "decision",
      target: { type: "message", messageId: "m-1", threadId: "th-1" },
    });

    expect(inboxTargetPath("team-1", message)).toBe(threadPath("team-1", "th-1"));
    expect(inboxTargetPath("team-1", at(5))).toBe(activityPath("team-1"));
  });

  it("权限请求落到任务详情（权限弹窗在那里）", () => {
    const permission = item({
      id: "permission:1",
      category: "permission",
      taskId: "t-9",
      target: { type: "permission", runId: "run-1", requestId: "req-1" },
    });

    expect(inboxTargetPath("team-1", permission)).toBe(taskPath("team-1", "t-9"));
  });

  it("通知优先落到任务，其次线程，最后活动", () => {
    const withTask: Notification = {
      id: "n-1",
      teamId: "team-1",
      category: "task_review",
      categoryLabel: "任务待验收",
      title: "任务待验收",
      body: "",
      taskId: "t-1",
      taskKey: "LD-008",
      messageId: null,
      threadId: null,
      runId: null,
      roleId: null,
      occurredAt: "2026-10-21T00:00:00Z",
    };
    const withThread: Notification = { ...withTask, id: "n-2", taskId: null, threadId: "th-1" };
    const bare: Notification = { ...withTask, id: "n-3", taskId: null, threadId: null };

    expect(notificationTargetPath("team-1", withTask)).toBe(taskPath("team-1", "t-1"));
    expect(notificationTargetPath("team-1", withThread)).toBe(threadPath("team-1", "th-1"));
    expect(notificationTargetPath("team-1", bare)).toBe(activityPath("team-1"));
  });
});

describe("inboxActionLabel", () => {
  it("每个类别都有对应的操作文案", () => {
    expect(inboxActionLabel(at(0))).toBe("去验收");
    expect(inboxActionLabel(at(1))).toBe("查看任务详情");
    expect(inboxActionLabel(at(3))).toBe("去答复权限");
    expect(inboxActionLabel(at(4))).toBe("查看消息线程");
    expect(inboxActionLabel(at(5))).toBe("查看活动");
  });
});

describe("inboxItemTone", () => {
  it("需要拍板与执行异常用告警底色，其余保持中性", () => {
    expect(inboxItemTone("blocked")).toBe("warn");
    expect(inboxItemTone("decision")).toBe("warn");
    expect(inboxItemTone("review")).toBe("plain");
    expect(inboxItemTone("permission")).toBe("plain");
    expect(inboxItemTone("leader_denied")).toBe("plain");
  });
});
