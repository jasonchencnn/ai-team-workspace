// 查询 key 约定：[资源名, ...参数]。同一资源的 key 全部在这里定义，
// 页面不要手写字符串 key；变更后用前缀失效，例如 invalidateQueries({ queryKey: queryKeys.tasks(teamId) })。
import type { EventQuery } from "../bindings/EventQuery.js";
import type { ListThreadsQuery } from "../bindings/ListThreadsQuery.js";
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
  /** 团队的轻量任务列表（编号与标题）；活动页的任务下拉框用。 */
  taskOptions: (teamId: string) => ["teams", teamId, "tasks", "options"] as const,
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
  /** Leader 接入配置（团队空间页的「接入 Leader」卡片）。 */
  leaderIntegration: (teamId: string) => ["teams", teamId, "leader-integration"] as const,
  /** 岗位消息的全部查询前缀：线程列表与线程详情都挂在它下面，失效时一起刷新。 */
  messageThreads: (teamId: string) => ["teams", teamId, "message-threads"] as const,
  /** 线程列表（带岗位与任务筛选）；前缀可用于一次失效所有筛选下的列表。 */
  messageThreadLists: (teamId: string) => ["teams", teamId, "message-threads", "list"] as const,
  messageThreadList: (teamId: string, query: ListThreadsQuery) =>
    ["teams", teamId, "message-threads", "list", query] as const,
  /** 一个线程及其中的消息。 */
  messageThread: (teamId: string, threadId: string) =>
    ["teams", teamId, "message-threads", "detail", threadId] as const,
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
  /** 某岗位进行中的执行；挂在岗位查询之下，换绑流程与待生效提示共用。 */
  roleActiveRuns: (teamId: string, roleId: string) =>
    ["teams", teamId, "roles", "active-runs", roleId] as const,
  /** 换绑期间待投递的消息；挂在岗位查询之下。 */
  rolePendingMessages: (teamId: string, roleId: string) =>
    ["teams", teamId, "roles", "pending-messages", roleId] as const,
  /** 换绑时的启动包预览文本；挂在岗位查询之下。 */
  roleHandoffPreview: (teamId: string, roleId: string) =>
    ["teams", teamId, "roles", "handoff-preview", roleId] as const,
  /** 待我处理：按类别聚合需要 Human 判断的事项。 */
  inbox: (teamId: string) => ["teams", teamId, "inbox"] as const,
  /** 决策列表：按团队与任务（或消息）过滤。 */
  decisions: (teamId: string, taskId: string) =>
    ["teams", teamId, "inbox", "decisions", taskId] as const,
  /** 通知订阅设置。 */
  notificationSettings: (teamId: string) => ["teams", teamId, "notification-settings"] as const,
} as const;
