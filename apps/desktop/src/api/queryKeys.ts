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
  events: (teamId: string, query: EventQuery) => ["teams", teamId, "events", query] as const,
  roleTemplates: ["role-templates"] as const,
  acpPresets: ["acp-presets"] as const,
  taskStatuses: ["task-statuses"] as const,
} as const;
