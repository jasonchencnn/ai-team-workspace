// 团队自动化设置（D43）的纯函数：表单文本与设置之间的转换。
import type { TeamAutomation } from "../../bindings/TeamAutomation.js";
import type { UpdateTeamAutomationInput } from "../../bindings/UpdateTeamAutomationInput.js";

/** 自治级别的说明（下拉选项）。 */
export const AUTONOMY_LEVELS: { value: number; label: string; hint: string }[] = [
  {
    value: 0,
    label: "0：合并由我来点",
    hint: "Leader 只能评审与退回，通过（合并）只能由你在应用里点。",
  },
  {
    value: 1,
    label: "1：只合并文档或测试",
    hint: "改动只有文档（Markdown、docs/）或测试时 Leader 可以直接合并，其余留给你。",
  },
  {
    value: 2,
    label: "2：闸门全过就合并",
    hint: "无冲突、不碰受保护路径、检查命令通过时 Leader 可以直接合并。",
  },
];

/** 表单草稿：列表类设置按「每行一项」编辑。 */
export interface AutomationDraft {
  autonomyLevel: number;
  checkCommand: string;
  protectedPaths: string;
  allowCommands: string;
  denyCommands: string;
  notifyLeader: boolean;
}

/** 设置 → 草稿。 */
export function toDraft(settings: TeamAutomation): AutomationDraft {
  return {
    autonomyLevel: settings.autonomyLevel,
    checkCommand: settings.checkCommand ?? "",
    protectedPaths: settings.protectedPaths.join("\n"),
    allowCommands: settings.allowCommands.join("\n"),
    denyCommands: settings.denyCommands.join("\n"),
    notifyLeader: settings.notifyLeader,
  };
}

/** 每行一项：去掉首尾空白与空行（重复项由内核去掉）。 */
export function lines(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

/** 草稿 → 修改请求。 */
export function toInput(teamId: string, draft: AutomationDraft): UpdateTeamAutomationInput {
  const check = draft.checkCommand.trim();
  return {
    teamId,
    autonomyLevel: draft.autonomyLevel,
    checkCommand: check.length === 0 ? null : check,
    protectedPaths: lines(draft.protectedPaths),
    allowCommands: lines(draft.allowCommands),
    denyCommands: lines(draft.denyCommands),
    notifyLeader: draft.notifyLeader,
  };
}
