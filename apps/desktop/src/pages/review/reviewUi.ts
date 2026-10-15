// 验收页的展示辅助：统一 diff 文本解析、按钮可用状态、文案。
// 能否通过 / 退回 / 恢复一律取自 get_review 返回的字段，这里只做组合与格式化，不复制后端规则。
import type { ReviewView } from "../../bindings/ReviewView.js";

export type DiffRowKind = "hunk" | "context" | "add" | "del" | "note";

export interface DiffRow {
  kind: DiffRowKind;
  /** 旧文件行号（新增行、hunk、note 为 null）。 */
  oldNo: number | null;
  /** 新文件行号（删除行、hunk、note 为 null）。 */
  newNo: number | null;
  /** 行内容：add / del / context 已去掉行首的 +、-、空格标记；hunk 为整行 `@@ … @@`。 */
  text: string;
}

const HUNK_HEADER = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

/**
 * 解析 `git diff` 的统一 diff 文本为逐行渲染用的行。第一个 `@@` 之前的头部（diff --git、index、---、+++ 等）
 * 忽略；`\ No newline at end of file` 作为说明行保留。
 */
export function parseUnifiedDiff(patch: string): DiffRow[] {
  const rows: DiffRow[] = [];
  let inHunk = false;
  let oldNo = 0;
  let newNo = 0;
  const lines = patch.split("\n");
  // 末尾换行产生的最后一个空串不是内容行。
  if (lines.length > 0 && lines[lines.length - 1] === "") {
    lines.pop();
  }
  for (const line of lines) {
    const header = HUNK_HEADER.exec(line);
    if (header !== null) {
      inHunk = true;
      oldNo = Number(header[1]);
      newNo = Number(header[2]);
      rows.push({ kind: "hunk", oldNo: null, newNo: null, text: line });
      continue;
    }
    if (!inHunk) {
      continue;
    }
    if (line.startsWith("+")) {
      rows.push({ kind: "add", oldNo: null, newNo, text: line.slice(1) });
      newNo += 1;
    } else if (line.startsWith("-")) {
      rows.push({ kind: "del", oldNo, newNo: null, text: line.slice(1) });
      oldNo += 1;
    } else if (line.startsWith("\\")) {
      rows.push({ kind: "note", oldNo: null, newNo: null, text: line.slice(1).trim() });
    } else {
      rows.push({
        kind: "context",
        oldNo,
        newNo,
        text: line.startsWith(" ") ? line.slice(1) : line,
      });
      oldNo += 1;
      newNo += 1;
    }
  }
  return rows;
}

/** 「+12 −3」中的数字格式：千分位。 */
export function formatCount(value: number): string {
  return value.toLocaleString("en-US");
}

/** 验收页当前应显示的模式。 */
export type ReviewMode = "none" | "pending" | "conflict";

export interface ReviewActionState {
  mode: ReviewMode;
  /** 通过按钮可用（后端 canApprove）。 */
  approveEnabled: boolean;
  approveLabel: string;
  /** 退回按钮可用：后端 canReject 且意见非空。 */
  rejectEnabled: boolean;
  /** 退回按钮不可用时的提示；可用时为 null。 */
  rejectHint: string | null;
  resolveEnabled: boolean;
  rerunEnabled: boolean;
  /** 后端给出的不能操作的原因（中文）。 */
  problems: string[];
}

/** 从 get_review 数据与退回意见输入推出按钮状态。 */
export function reviewActionState(view: ReviewView, comment: string): ReviewActionState {
  const mode: ReviewMode = view.conflict ? "conflict" : view.run === null ? "none" : "pending";
  const commentEmpty = comment.trim() === "";
  const approveLabel =
    view.approveAction === "merge" ? `通过并合并到 ${view.baseBranch ?? "当前分支"}` : "通过";
  let rejectHint: string | null = null;
  if (mode === "pending") {
    if (!view.canReject) {
      rejectHint = "现在不能退回，原因见上方说明";
    } else if (commentEmpty) {
      rejectHint = "意见为空时不能退回";
    }
  }
  return {
    mode,
    approveEnabled: mode === "pending" && view.canApprove,
    approveLabel,
    rejectEnabled: mode === "pending" && view.canReject && !commentEmpty,
    rejectHint,
    resolveEnabled: mode === "conflict" && view.canResolveManually,
    rerunEnabled: mode === "conflict" && view.canRerun,
    problems: view.problems,
  };
}

/** 手动合并执行分支用的命令（复制给 Human 在仓库里执行）。 */
export function mergeCommand(baseBranch: string | null, branch: string | null): string | null {
  if (branch === null) {
    return null;
  }
  const merge = `git merge --no-ff ${branch}`;
  return baseBranch === null ? merge : `git switch ${baseBranch} && ${merge}`;
}

/** 合并 / 标记完成后自动转 READY 的下游任务。后端字段（`advancedTasks`）合入前恒为空；合入后只需改这里。 */
export interface AdvancedTask {
  id: string;
  key: string;
  title: string;
}

export function advancedTasksOf(result: object): AdvancedTask[] {
  const value = (result as { advancedTasks?: unknown }).advancedTasks;
  return Array.isArray(value) ? (value as AdvancedTask[]) : [];
}
