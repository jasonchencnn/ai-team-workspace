// 任务前缀自动填入的竞态回归，两层：
// 1. 异步建议返回时只能覆盖自动值，不能覆盖用户手填的值——判定逻辑抽在 roleForm.tsx 的
//    纯函数 resolveAutoFillPrefix 里，这里直接按返回值断言；
// 2. 建议请求乱序返回时只采用最新名称的结果——判定逻辑抽在 runLatestSuggestion 里，
//    这里用可手动决定何时返回的假内核复现真实时序。
import { describe, expect, it } from "vitest";

import {
  EMPTY_PREFIX_FILL,
  commitPendingFillStates,
  createPrefixSuggestionGate,
  resolveAutoFillPrefix,
  runLatestSuggestion,
  type PrefixFillState,
} from "../src/pages/roles/roleForm.js";

/** 模拟一行的真实时序：建议到达时传入那一刻的前缀值，并把行状态带到下一步。 */
function settle(
  state: PrefixFillState,
  currentPrefix: string,
  suggested: string,
): { prefix: string; changed: boolean; state: PrefixFillState } {
  const result = resolveAutoFillPrefix(state, currentPrefix, suggested);
  return result;
}

describe("任务前缀自动填入判定", () => {
  it("前缀为空时写入建议值", () => {
    const result = settle(EMPTY_PREFIX_FILL, "", "R1");

    expect(result.prefix).toBe("R1");
    expect(result.changed).toBe(true);
    expect(result.state).toEqual({ autoFilled: "R1", manual: false });
  });

  it("前缀仍是上次自动填入的值时跟随新建议更新（名称变化）", () => {
    const state = settle(EMPTY_PREFIX_FILL, "", "R1").state;

    const result = settle(state, "R1", "R2");

    expect(result.prefix).toBe("R2");
    expect(result.changed).toBe(true);
    expect(result.state).toEqual({ autoFilled: "R2", manual: false });
  });

  it("建议返回前用户已手填前缀时不覆盖手填值", () => {
    // 失焦那一刻前缀还是空的（请求在途）；用户随后手填 X。
    const result = settle(EMPTY_PREFIX_FILL, "X", "R1");

    expect(result.prefix).toBe("X");
    expect(result.changed).toBe(false);
    expect(result.state).toEqual({ autoFilled: null, manual: true });
  });

  it("用户手填后名称再次变化也不再自动覆盖", () => {
    const state = settle(EMPTY_PREFIX_FILL, "X", "R1").state;

    const result = settle(state, "X", "R2");

    expect(result.prefix).toBe("X");
    expect(result.changed).toBe(false);
    expect(result.state.manual).toBe(true);
  });

  it("用户改过的自动值再手填别的值后按手填处理", () => {
    const auto = settle(EMPTY_PREFIX_FILL, "", "R1").state;

    const result = settle(auto, "ARCH", "R2");

    expect(result.prefix).toBe("ARCH");
    expect(result.changed).toBe(false);
    expect(result.state).toEqual({ autoFilled: "R1", manual: true });
  });

  it("用户自己清空前缀后重新接受建议", () => {
    const state = settle(EMPTY_PREFIX_FILL, "X", "R1").state;

    const result = settle(state, "", "R3");

    expect(result.prefix).toBe("R3");
    expect(result.changed).toBe(true);
    expect(result.state).toEqual({ autoFilled: "R3", manual: false });
  });

  it("建议与现有值相同时不产生多余改动", () => {
    const state = settle(EMPTY_PREFIX_FILL, "", "R1").state;

    const result = settle(state, "R1", "R1");

    expect(result.prefix).toBe("R1");
    expect(result.changed).toBe(false);
  });

  it("拒绝后重复判定幂等，React 重放 updater 不会跑偏", () => {
    const first = settle(EMPTY_PREFIX_FILL, "X", "R1");
    const second = settle(first.state, "X", "R1");

    expect(second).toEqual(first);

    const applied = settle(EMPTY_PREFIX_FILL, "", "R1");
    const reapplied = settle(applied.state, "", "R1");

    expect(reapplied).toEqual(applied);
  });
});

/** 让 `await` 之后的续体跑完：一次 setTimeout 会把挂起的微任务全部排空。 */
function drainMicrotasks(): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, 0);
  });
}

/**
 * 可手动决定何时返回的假内核：请求按名称登记，返回顺序完全由测试指定，
 * 用来稳定复现「先发后到」和「后发先到」两种乱序。
 */
function deferredKernel() {
  interface Pending {
    resolve: (value: string) => void;
    reject: (reason: unknown) => void;
  }
  const inflight = new Map<string, Pending>();

  const request = (name: string) =>
    new Promise<string>((resolve, reject) => {
      inflight.set(name, { resolve, reject });
    });

  /** 已经发出、但还没决定返回结果的请求名称，按发出顺序。 */
  const issued = (): string[] => [...inflight.keys()];

  /** 让某个在途请求按指定结果返回（传 Error 即失败），然后等它处理完。 */
  const settle = async (name: string, result: string | Error): Promise<void> => {
    const pending = inflight.get(name);
    expect(pending, `请求尚未发出：${name}`).toBeDefined();
    if (result instanceof Error) {
      pending?.reject(result);
    } else {
      pending?.resolve(result);
    }
    await drainMicrotasks();
  };

  return { request, issued, settle };
}

describe("建议请求乱序返回", () => {
  it("后发先到：最新名称的建议先返回并生效，先发后到的旧响应全部丢弃", async () => {
    // QA 复现路径：连续三次改名失焦，内核返回顺序与发出顺序相反。
    const gate = createPrefixSuggestionGate();
    const kernel = deferredKernel();
    const applied: string[] = [];
    const commit = (suggested: string) => applied.push(suggested);

    const first = runLatestSuggestion(gate, "row-1", "Stage One", kernel.request, commit);
    const second = runLatestSuggestion(gate, "row-1", "Stage Two", kernel.request, commit);
    const third = runLatestSuggestion(gate, "row-1", "Final Delivery", kernel.request, commit);
    expect(kernel.issued()).toEqual(["Stage One", "Stage Two", "Final Delivery"]);

    // 最新那次先回来：应当立刻写入 FD。
    await kernel.settle("Final Delivery", "FD");
    await third;
    expect(applied).toEqual(["FD"]);

    // 最早两次迟到：都对应旧名称，必须丢弃，不能把 FD 退回 SO / ST。
    await kernel.settle("Stage One", "SO");
    await first;
    await kernel.settle("Stage Two", "ST");
    await second;

    expect(applied).toEqual(["FD"]);
  });

  it("先发后到：请求重叠且按发出顺序返回时，只有最新名称的结果被采用", async () => {
    const gate = createPrefixSuggestionGate();
    const kernel = deferredKernel();
    const applied: string[] = [];
    const commit = (suggested: string) => applied.push(suggested);

    const first = runLatestSuggestion(gate, "row-1", "Stage One", kernel.request, commit);
    const second = runLatestSuggestion(gate, "row-1", "Final Delivery", kernel.request, commit);

    // 先发的那次先回来：此时已按更新名称重新问过内核，SO 直接过期。
    await kernel.settle("Stage One", "SO");
    await first;
    expect(applied).toEqual([]);

    await kernel.settle("Final Delivery", "FD");
    await second;
    expect(applied).toEqual(["FD"]);
  });

  it("请求不重叠时前缀跟随最新一次名称", async () => {
    const gate = createPrefixSuggestionGate();
    const kernel = deferredKernel();
    const applied: string[] = [];
    const commit = (suggested: string) => applied.push(suggested);

    const first = runLatestSuggestion(gate, "row-1", "Stage One", kernel.request, commit);
    await kernel.settle("Stage One", "SO");
    await first;
    expect(applied).toEqual(["SO"]);

    // 上一次已完全落地，再改名：正常跟随新名称。
    const second = runLatestSuggestion(gate, "row-1", "Final Delivery", kernel.request, commit);
    await kernel.settle("Final Delivery", "FD");
    await second;
    expect(applied).toEqual(["SO", "FD"]);
  });

  it("不同岗位行的请求互不作废", async () => {
    const gate = createPrefixSuggestionGate();
    const kernel = deferredKernel();
    const applied: { row: string; prefix: string }[] = [];

    const rowA = runLatestSuggestion(gate, "row-a", "Stage One", kernel.request, (prefix) =>
      applied.push({ row: "row-a", prefix }),
    );
    const rowB = runLatestSuggestion(gate, "row-b", "Colorist", kernel.request, (prefix) =>
      applied.push({ row: "row-b", prefix }),
    );

    // 行 B 后发起但先返回，不影响行 A 自己的请求仍然有效。
    await kernel.settle("Colorist", "CO");
    await rowB;
    await kernel.settle("Stage One", "SO");
    await rowA;

    expect(applied).toEqual([
      { row: "row-b", prefix: "CO" },
      { row: "row-a", prefix: "SO" },
    ]);
  });

  it("建议请求失败时保持前缀原样，不写回也不抛出", async () => {
    const gate = createPrefixSuggestionGate();
    const kernel = deferredKernel();
    const applied: string[] = [];

    const pending = runLatestSuggestion(gate, "row-1", "Stage One", kernel.request, (suggested) =>
      applied.push(suggested),
    );
    await kernel.settle("Stage One", new Error("invoke failed"));

    await expect(pending).resolves.toBeUndefined();
    expect(applied).toEqual([]);
  });

  it("乱序返回与手填保护叠加：过期响应不写，用户手填值也不被最新建议冲掉", async () => {
    const gate = createPrefixSuggestionGate();
    const kernel = deferredKernel();
    const fillState: PrefixFillState[] = [EMPTY_PREFIX_FILL];
    const commit = (suggested: string) => {
      // 模拟前端：建议返回那一刻才读这一行的最新前缀。
      const resolved = resolveAutoFillPrefix(fillState[0] ?? EMPTY_PREFIX_FILL, "MAN", suggested);
      fillState[0] = resolved.state;
    };

    const first = runLatestSuggestion(gate, "row-1", "Stage One", kernel.request, commit);
    const second = runLatestSuggestion(gate, "row-1", "Final Delivery", kernel.request, commit);

    // 用户在等待期间手填了 MAN：先到的旧响应不得写入。
    await kernel.settle("Stage One", "SO");
    await first;
    expect(fillState[0]).toEqual(EMPTY_PREFIX_FILL);

    // 最新一次建议也不该覆盖手填值。
    await kernel.settle("Final Delivery", "FD");
    await second;
    expect(fillState[0]).toEqual({ autoFilled: null, manual: true });
  });
});

/**
 * 一行的写入管线，和 `useTaskPrefixSuggester` 内部一致：
 * 判定在 updater 里做、只幂等地记下待落盘状态，行状态由渲染之外统一落盘。
 *
 * `replay` 模拟 React 重放 updater（StrictMode 下同一个 updater 会被调用两次）。
 */
function createRowWriter() {
  const fillState: Record<string, PrefixFillState> = {};
  let pending: Record<string, PrefixFillState> = {};

  const write = (key: string, currentPrefix: string, suggested: string, replay = 1): string => {
    let prefix = currentPrefix;
    for (let i = 0; i < replay; i += 1) {
      const resolved = resolveAutoFillPrefix(
        fillState[key] ?? EMPTY_PREFIX_FILL,
        currentPrefix,
        suggested,
      );
      pending[key] = resolved.state;
      prefix = resolved.prefix;
    }
    commitPendingFillStates(fillState, pending);
    pending = {};
    return prefix;
  };

  return { write, fillState };
}

describe("updater 被 React 重放时不误判用户手填", () => {
  it("连续改名且每次写入都被重放两次，前缀仍跟随最新名称", () => {
    const row = createRowWriter();

    // 若把 autoFilled 写在 updater 里，第二次重放会读到刚写入的 "SO"，
    // 把自动值误判成用户手填，于是这里会卡在 "SO"（QA 复现到的 SO）。
    expect(row.write("draft-2", "", "SO", 2)).toBe("SO");
    expect(row.write("draft-2", "SO", "ST", 2)).toBe("ST");
    expect(row.write("draft-2", "ST", "FD", 2)).toBe("FD");
    expect(row.fillState["draft-2"]).toEqual({ autoFilled: "FD", manual: false });
  });

  it("重放不影响真正的手填保护：用户填的值仍然优先", () => {
    const row = createRowWriter();
    row.write("draft-2", "", "SO", 2);

    // 用户在等待期间手填 MAN：重放多少次都不该被建议冲掉。
    expect(row.write("draft-2", "MAN", "ST", 2)).toBe("MAN");
    expect(row.fillState["draft-2"]).toEqual({ autoFilled: "SO", manual: true });
  });

  it("落盘后已有行状态不受后续空记录影响", () => {
    const fillState: Record<string, PrefixFillState> = {};
    const pending: Record<string, PrefixFillState> = {};

    pending["draft-1"] = { autoFilled: "ED", manual: false };
    commitPendingFillStates(fillState, pending);
    expect(fillState["draft-1"]).toEqual({ autoFilled: "ED", manual: false });

    // 没有待落盘记录时是空操作，已落盘的状态不会被清空。
    commitPendingFillStates(fillState, {});
    expect(fillState["draft-1"]).toEqual({ autoFilled: "ED", manual: false });
  });
});
