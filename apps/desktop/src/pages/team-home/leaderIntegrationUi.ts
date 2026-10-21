// 「接入 Leader」卡片的纯逻辑：能做什么 / 不能做什么、三段配置的文案、复制按钮状态与提示。
// 页面只渲染这里算出来的结果；判断都在这里，才能在不开界面的情况下验证「哪段配置复制什么」。
import type { LeaderIntegration } from "../../bindings/LeaderIntegration.js";

/** Leader 通过 teamspace-mcp 能做的事（D26 的白名单范围）。 */
export const LEADER_CAN_DO: readonly string[] = [
  "查看团队、岗位、Harness、看板与任务详情",
  "创建与编辑任务、设置依赖、指定负责岗位",
  "开始与取消执行，查看执行进度与结果",
  "查看验收、退回并写意见",
  "查询等待答复的权限请求",
];

/** 只能由你在应用里做的事（Leader 无权执行）。 */
export const LEADER_CANNOT_DO: readonly string[] = [
  "通过验收并合并到主分支",
  "答复 Agent 的权限请求",
  "换绑或新建 Harness、岗位与团队",
  "修改权限与岗位配置",
];

/** 一段可复制的配置。 */
export interface ConfigSection {
  key: "claude" | "mcpJson" | "codex";
  title: string;
  hint: string;
  text: string;
}

/** 三段配置：Claude Code 命令、工作空间 `.mcp.json` 片段、Codex `config.toml` 片段。 */
export function configSections(integration: LeaderIntegration): ConfigSection[] {
  return [
    {
      key: "claude",
      title: "Claude Code",
      hint: "在工作空间目录执行这一行登记 MCP 服务；登记后用 /mcp 确认已连接。",
      text: integration.claudeCommand,
    },
    {
      key: "mcpJson",
      title: "工作空间 .mcp.json",
      hint: "Claude Code 的项目级配置：已有 .mcp.json 时把 teamspace 一项放进 mcpServers 里即可。",
      text: integration.mcpJson,
    },
    {
      key: "codex",
      title: "Codex（~/.codex/config.toml）",
      hint: "追加到 Codex 的配置文件里，与已有的其他 MCP 服务并存。",
      text: integration.codexToml,
    },
  ];
}

/** 复制按钮的状态。 */
export type CopyState = "idle" | "ok" | "failed";

/** 复制按钮上的文字。 */
export function copyLabel(state: CopyState): string {
  if (state === "ok") {
    return "已复制";
  }
  if (state === "failed") {
    return "复制失败";
  }
  return "复制";
}

/** 复制失败时的补充说明。 */
export const COPY_FAILED_HINT = "无法写入剪贴板，请手动选中上面的文本复制。";

/** Git 工作空间的提示：`.mcp.json` 会出现在 Git 改动里。 */
export function gitNotice(gitWorkspace: boolean): string | null {
  return gitWorkspace ? "此文件会出现在 Git 改动中，可加入 .gitignore 或提交。" : null;
}

/** 写入工作空间 `.mcp.json` 前的确认说明。 */
export function writeConfirmText(integration: LeaderIntegration): string {
  return `将把 mcpServers.teamspace 写入 ${integration.workspacePath}/.mcp.json。已有文件只合并 teamspace 一项，其余内容保留（文件会按规范缩进重写）。`;
}

/** 写入成功后的提示。 */
export function writeDoneText(path: string, created: boolean): string {
  return created ? `已新建 ${path}` : `已把 teamspace 一项合并进 ${path}`;
}
