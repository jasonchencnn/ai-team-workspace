// 查询 key 约定：[资源名, ...参数]。同一资源的 key 全部在这里定义，
// 页面不要手写字符串 key；变更后用前缀失效，例如 invalidateQueries({ queryKey: queryKeys.tasks(teamId) })。
import type { EventQuery } from "../bindings/EventQuery.js";
import type { TaskBoardQuery } from "../bindings/TaskBoardQuery.js";

export const queryKeys = {
  teams: ["teams"] as const,
  teamOverview: (teamId: string) => ["teams", teamId, "overview"] as const,
  roles: (teamId: string) => ["teams", teamId, "roles"] as const,
  harnesses: (teamId: string) => ["teams", teamId, "harnesses"] as const,
  /** 某团队全部任务相关查询的前缀（看板、详情）。 */
  tasks: (teamId: string) => ["teams", teamId, "tasks"] as const,
  taskBoard: (teamId: string, query: TaskBoardQuery) =>
    ["teams", teamId, "tasks", "board", query] as const,
  task: (teamId: string, taskId: string) => ["teams", teamId, "tasks", "detail", taskId] as const,
  /**
   * 任务的全部执行记录。挂在 `task` 之下，执行状态变化时随任务详情一并失效。
   */
  runs: (teamId: string, taskId: string) =>
    ["teams", teamId, "tasks", "detail", taskId, "runs"] as const,
  /** 验收页数据；挂在 `task` 之下，任务相关查询失效时一并刷新。 */
  review: (teamId: string, taskId: string) =>
    ["teams", teamId, "tasks", "detail", taskId, "review"] as const,
  /** 验收页某个文件的 diff（按执行 ID 区分，退回续接后是新的执行）。 */
  reviewPatch: (runId: string, path: string) => ["review-patch", runId, path] as const,
  /** 某团队全部活动查询的前缀。 */
  teamEvents: (teamId: string) => ["teams", teamId, "events"] as const,
  events: (teamId: string, query: EventQuery) => ["teams", teamId, "events", query] as const,
  /** 共享文件页的工作空间 Markdown 列表。挂在团队之下，随「刷新文件列表」重新取。 */
  markdownFiles: (teamId: string) => ["teams", teamId, "markdown-files"] as const,
  markdownFile: (teamId: string, path: string) =>
    ["teams", teamId, "markdown-files", "doc", path] as const,
  /** 引用了某个文件的任务，挂在该文件的查询之下。 */
  fileTaskRefs: (teamId: string, path: string) =>
    ["teams", teamId, "markdown-files", "doc", path, "task-refs"] as const,
  roleTemplates: ["role-templates"] as const,
  acpPresets: ["acp-presets"] as const,
  taskStatuses: ["task-statuses"] as const,
} as const;
