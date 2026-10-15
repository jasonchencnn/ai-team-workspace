// 权限弹窗按钮文案：按 option.kind 映射成中文，未知种类回退到 Agent 提供的 name。
import { describe, expect, it } from "vitest";

import type { PermissionOption } from "../src/bindings/PermissionOption.js";
import { permissionOptionLabel } from "../src/pages/task-detail/run/runUi.js";

function option(kind: string, name: string): PermissionOption {
  return { optionId: `opt-${kind}`, name, kind };
}

describe("权限选项按钮文案", () => {
  it("allow_once 显示「允许一次」，不管 Agent 叫什么", () => {
    expect(permissionOptionLabel(option("allow_once", "Allow once"))).toBe("允许一次");
  });

  it("allow_always 显示「本次执行内总是允许」", () => {
    expect(permissionOptionLabel(option("allow_always", "Always allow"))).toBe(
      "本次执行内总是允许",
    );
  });

  it("reject_once 显示「拒绝」", () => {
    expect(permissionOptionLabel(option("reject_once", "Reject"))).toBe("拒绝");
  });

  it("reject_always 显示「总是拒绝」", () => {
    expect(permissionOptionLabel(option("reject_always", "Always reject"))).toBe("总是拒绝");
  });

  it("未知种类回退到 Agent 提供的 name", () => {
    expect(permissionOptionLabel(option("some_future_kind", "Agent 自定义选项"))).toBe(
      "Agent 自定义选项",
    );
  });

  it("原型链上的属性名不会被当成已知种类", () => {
    expect(permissionOptionLabel(option("toString", "原名"))).toBe("原名");
    expect(permissionOptionLabel(option("__proto__", "原名"))).toBe("原名");
  });
});
