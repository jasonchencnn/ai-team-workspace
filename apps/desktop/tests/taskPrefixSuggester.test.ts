// 任务前缀自动填入的竞态回归：异步建议返回时只能覆盖自动值，不能覆盖用户手填的值。
// 判定逻辑抽在 roleForm.tsx 的纯函数 resolveAutoFillPrefix 里，这里直接按返回值断言。
import { describe, expect, it } from "vitest";

import {
  EMPTY_PREFIX_FILL,
  resolveAutoFillPrefix,
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
