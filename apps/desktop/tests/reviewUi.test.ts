// 验收页纯逻辑：统一 diff 文本解析（增删行与行号）和按钮可用状态。
import { describe, expect, it } from "vitest";

import type { ReviewView } from "../src/bindings/ReviewView.js";
import { mergeCommand, parseUnifiedDiff, reviewActionState } from "../src/pages/review/reviewUi.js";

const PATCH = [
  "diff --git a/a.txt b/a.txt",
  "index 1111111..2222222 100644",
  "--- a/a.txt",
  "+++ b/a.txt",
  "@@ -1,3 +1,4 @@ title",
  " one",
  "-two",
  "--dash",
  "+TWO",
  "+three",
  " four",
  "\\ No newline at end of file",
  "@@ -20 +21 @@",
  "-x",
  "+y",
  "",
].join("\n");

describe("parseUnifiedDiff", () => {
  const rows = parseUnifiedDiff(PATCH);

  it("跳过文件头，保留 hunk 头", () => {
    expect(rows[0]).toEqual({
      kind: "hunk",
      oldNo: null,
      newNo: null,
      text: "@@ -1,3 +1,4 @@ title",
    });
    expect(rows.some((row) => row.text.startsWith("diff --git"))).toBe(false);
  });

  it("增删行去掉标记并带上各自的行号", () => {
    expect(rows[1]).toEqual({ kind: "context", oldNo: 1, newNo: 1, text: "one" });
    expect(rows[2]).toEqual({ kind: "del", oldNo: 2, newNo: null, text: "two" });
    // 内容以 -- 开头的删除行不能被当成文件头
    expect(rows[3]).toEqual({ kind: "del", oldNo: 3, newNo: null, text: "-dash" });
    expect(rows[4]).toEqual({ kind: "add", oldNo: null, newNo: 2, text: "TWO" });
    expect(rows[5]).toEqual({ kind: "add", oldNo: null, newNo: 3, text: "three" });
    expect(rows[6]).toEqual({ kind: "context", oldNo: 4, newNo: 4, text: "four" });
  });

  it("保留无换行说明，并在下一个 hunk 重置行号", () => {
    expect(rows[7]).toEqual({
      kind: "note",
      oldNo: null,
      newNo: null,
      text: "No newline at end of file",
    });
    expect(rows[8]?.kind).toBe("hunk");
    expect(rows[9]).toEqual({ kind: "del", oldNo: 20, newNo: null, text: "x" });
    expect(rows[10]).toEqual({ kind: "add", oldNo: null, newNo: 21, text: "y" });
    expect(rows).toHaveLength(11);
  });

  it("空文本没有行", () => {
    expect(parseUnifiedDiff("")).toEqual([]);
  });
});

function view(overrides: Partial<ReviewView>): ReviewView {
  return {
    task: {} as ReviewView["task"],
    run: {} as ReviewView["run"],
    summary: null,
    isolation: "worktree",
    baseBranch: "main",
    branch: "teamspace/FE-001-r1",
    files: [],
    filesProblem: null,
    approveAction: "merge",
    canApprove: true,
    canReject: true,
    problems: [],
    rejections: [],
    conflict: false,
    canResolveManually: false,
    canRerun: false,
    ...overrides,
  };
}

describe("reviewActionState", () => {
  it("通过按钮取决于后端 canApprove，标签带基线分支", () => {
    expect(reviewActionState(view({}), "").approveEnabled).toBe(true);
    expect(reviewActionState(view({}), "").approveLabel).toBe("通过并合并到 main");
    expect(reviewActionState(view({ canApprove: false }), "").approveEnabled).toBe(false);
    expect(reviewActionState(view({ approveAction: "complete" }), "").approveLabel).toBe("通过");
  });

  it("退回意见为空（含全空白）时禁用，填写后可用", () => {
    expect(reviewActionState(view({}), "").rejectEnabled).toBe(false);
    expect(reviewActionState(view({}), "").rejectHint).toBe("意见为空时不能退回");
    expect(reviewActionState(view({}), "  \n ").rejectEnabled).toBe(false);
    const filled = reviewActionState(view({}), "补一个用例");
    expect(filled.rejectEnabled).toBe(true);
    expect(filled.rejectHint).toBeNull();
  });

  it("后端不允许退回时即使有意见也禁用", () => {
    const state = reviewActionState(view({ canReject: false }), "补一个用例");
    expect(state.rejectEnabled).toBe(false);
    expect(state.rejectHint).not.toBeNull();
  });

  it("冲突状态只开放两个恢复按钮，各自取后端的字段", () => {
    const conflict = reviewActionState(
      view({ conflict: true, canApprove: false, canReject: false, canResolveManually: true }),
      "x",
    );
    expect(conflict.mode).toBe("conflict");
    expect(conflict.approveEnabled).toBe(false);
    expect(conflict.rejectEnabled).toBe(false);
    expect(conflict.resolveEnabled).toBe(true);
    expect(conflict.rerunEnabled).toBe(false);
  });

  it("没有待验收的执行时什么都不可用", () => {
    const state = reviewActionState(view({ run: null, canApprove: false, canReject: false }), "x");
    expect(state.mode).toBe("none");
    expect(state.approveEnabled).toBe(false);
    expect(state.rejectEnabled).toBe(false);
  });
});

describe("mergeCommand", () => {
  it("带基线分支时先切换", () => {
    expect(mergeCommand("main", "teamspace/FE-001-r1")).toBe(
      "git switch main && git merge --no-ff teamspace/FE-001-r1",
    );
    expect(mergeCommand(null, "b")).toBe("git merge --no-ff b");
    expect(mergeCommand("main", null)).toBeNull();
  });
});
