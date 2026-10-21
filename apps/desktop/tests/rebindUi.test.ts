// 换绑流程的纯逻辑：方式选项与默认值、待生效文案、候选实例筛选、启动包空态。
// 这些判定决定界面说什么、什么时候说什么，单元测试把它们锁住。
// 「哪些执行算进行中」由内核判定，前端不再列状态白名单，所以这里没有对应用例。
import { describe, expect, it } from "vitest";

import type { HarnessInstance } from "../src/bindings/HarnessInstance.js";
import type { PendingBinding } from "../src/bindings/PendingBinding.js";
import type { RebindMode } from "../src/bindings/RebindMode.js";
import {
  DEFAULT_REBIND_MODE,
  HANDOFF_EMPTY_TEXT,
  HANDOFF_EMPTY_TEXT_UNBOUND,
  REBIND_MODE_OPTIONS,
  REBIND_MODES,
  handoffEmptyText,
  pendingBindingSummary,
  pendingDeliveryNote,
  rebindCandidates,
  rebindModeLabel,
  rebindModeOption,
} from "../src/pages/roles/rebindUi.js";

/** 只用到界面关心字段的最小 Harness 实例。 */
function harness(id: string, name: string, archivedAt: string | null = null): HarnessInstance {
  return {
    id,
    teamId: "team-1",
    name,
    preset: "mock",
    command: "teamspace-mock-agent",
    args: [],
    envProfile: null,
    handshake: {
      status: "connected",
      agent: null,
      capabilities: null,
      authMethods: [],
      detail: null,
      protocolVersion: null,
      checkedAt: "2026-10-21T02:00:00Z",
    },
    createdAt: "2026-10-21T02:00:00Z",
    updatedAt: "2026-10-21T02:00:00Z",
    archivedAt,
  };
}

function pending(extra: Partial<PendingBinding> = {}): PendingBinding {
  return {
    id: "pending-1",
    roleId: "role-qa",
    roleName: "QA",
    harnessId: "harness-new",
    harnessName: "mock-new",
    previousHarnessId: "harness-old",
    previousHarnessName: "mock-old",
    mode: "after_current",
    requestedAt: "2026-10-21T02:31:00Z",
    activeRunId: "run-1",
    ...extra,
  };
}

describe("换绑方式", () => {
  it("三种方式与内核的 RebindMode 一一对应", () => {
    expect(REBIND_MODES).toEqual(["after_current", "cancel_and_rerun", "manual_takeover"]);
    expect(REBIND_MODE_OPTIONS.map((option) => option.mode)).toEqual(REBIND_MODES);
  });

  it("默认是「当前执行完成后更换」，且只有它是推荐项", () => {
    expect(DEFAULT_REBIND_MODE).toBe("after_current");
    const recommended = REBIND_MODE_OPTIONS.filter((option) => option.recommended);
    expect(recommended).toHaveLength(1);
    expect(recommended[0]?.mode).toBe("after_current");
  });

  it("每种方式都写清后果", () => {
    for (const option of REBIND_MODE_OPTIONS) {
      expect(option.label.length).toBeGreaterThan(0);
      expect(option.consequence.length).toBeGreaterThan(0);
    }
  });

  it("方式短名用于待生效摘要", () => {
    expect(rebindModeLabel("after_current")).toBe("当前执行完成后更换");
    expect(rebindModeLabel("cancel_and_rerun")).toBe("取消并重跑");
    expect(rebindModeLabel("manual_takeover")).toBe("人工接管");
  });

  it("取方式选项时未知方式退回默认项，不出现空标签", () => {
    const fallback = rebindModeOption("unknown" as RebindMode);
    expect(fallback.mode).toBe(DEFAULT_REBIND_MODE);
    expect(fallback.label.length).toBeGreaterThan(0);
  });
});

describe("候选实例", () => {
  it("排除当前绑定的实例与已归档的实例", () => {
    const list = [harness("h1", "当前"), harness("h2", "另一个"), harness("h3", "归档的", "x")];
    const candidates = rebindCandidates(list, "h1");
    expect(candidates.map((item) => item.id)).toEqual(["h2"]);
  });

  it("岗位没绑实例时，同团队未归档实例都能选", () => {
    const list = [harness("h1", "甲"), harness("h2", "乙")];
    expect(rebindCandidates(list, null).map((item) => item.id)).toEqual(["h1", "h2"]);
  });
});

describe("待生效文案", () => {
  it("写明旧实例、新实例、方式与发起时间", () => {
    const summary = pendingBindingSummary(pending(), "2026/10/21 10:31");
    expect(summary.title).toBe("待生效：mock-old → mock-new");
    expect(summary.detail).toBe("（当前执行完成后更换，2026/10/21 10:31）");
    expect(summary.oldName).toBe("mock-old");
    expect(summary.newName).toBe("mock-new");
    expect(summary.modeLabel).toBe("当前执行完成后更换");
  });

  it("岗位发起换绑时没有绑定过实例就写「未绑定」，不编造旧实例名", () => {
    const summary = pendingBindingSummary(
      pending({ previousHarnessId: null, previousHarnessName: null }),
      "2026/10/21 10:31",
    );
    expect(summary.oldName).toBe("未绑定");
    expect(summary.title).toBe("待生效：未绑定 → mock-new");
  });

  it("撤销与重跑的方式在摘要里也分得清", () => {
    const rerun = pendingBindingSummary(pending({ mode: "cancel_and_rerun" }), "2026/10/21 10:31");
    expect(rerun.detail).toContain("取消并重跑");
    const takeover = pendingBindingSummary(
      pending({ mode: "manual_takeover" }),
      "2026/10/21 10:31",
    );
    expect(takeover.detail).toContain("人工接管");
  });
});

describe("待投递消息说明", () => {
  it("写明入队时的绑定，并说明按生效后的绑定投递", () => {
    expect(pendingDeliveryNote("mock-old")).toBe("入队时绑定：mock-old，将按生效后的绑定投递");
  });

  it("入队时没有有效绑定就说清楚", () => {
    expect(pendingDeliveryNote(null)).toBe("入队时绑定：无有效绑定，将按生效后的绑定投递");
  });
});

describe("启动包空态", () => {
  it("已绑定但没有交接内容时提示不会附加交接", () => {
    expect(handoffEmptyText(true)).toBe(HANDOFF_EMPTY_TEXT);
    expect(HANDOFF_EMPTY_TEXT).toBe("该岗位在当前 Harness 上没有需要交接的内容，不会附加交接");
  });

  it("岗位没绑实例时说清楚是没有东西可交接", () => {
    expect(handoffEmptyText(false)).toBe(HANDOFF_EMPTY_TEXT_UNBOUND);
  });
});
