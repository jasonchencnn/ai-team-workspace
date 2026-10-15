// 活动与审计页共用的展示辅助：筛选换算与活动种类的显示名。
// 摘要与时间一律用后端返回的原文与时间，这里只负责把筛选控件的值翻译成后端查询条件。
import type { EventKind } from "../../bindings/EventKind.js";

/** 时间范围：按本地时区的自然日计算，与界面上的「今天」一致。 */
export type TimeRange = "today" | "7d" | "30d" | "all";

export const TIME_RANGES: { value: TimeRange; label: string }[] = [
  { value: "today", label: "今天" },
  { value: "7d", label: "近 7 天" },
  { value: "30d", label: "近 30 天" },
  { value: "all", label: "全部" },
];

/**
 * 活动种类的中文显示名。只用于下拉与标签，不参与任何判断；
 * 写成 `Record` 而不是 `Partial`，后端新增种类时这里会直接编译失败，避免漏译。
 */
export const KIND_LABELS: Record<EventKind, string> = {
  team_created: "创建团队",
  role_created: "新增岗位",
  role_updated: "编辑岗位",
  role_archived: "归档岗位",
  role_harness_bound: "绑定 Harness",
  role_harness_unbound: "解除绑定 Harness",
  harness_created: "注册 Harness",
  harness_updated: "编辑 Harness",
  harness_archived: "归档 Harness",
  harness_checked: "握手检查",
  task_created: "创建任务",
  task_updated: "编辑任务",
  task_status_changed: "任务状态变化",
  role_folder_error: "岗位文件夹异常",
  run_started: "开始执行",
  run_finished: "执行结束",
  run_interrupted: "执行中断",
  run_resumed: "续接执行",
};

/** 活动种类列表（下拉用），顺序与后端枚举一致。 */
export const EVENT_KINDS = Object.keys(KIND_LABELS) as EventKind[];

/** 操作者显示名：人还是 Teamspace 自己做主。 */
export const ACTOR_LABELS: Record<string, string> = {
  human: "你",
  system: "Teamspace",
};

/** 本地时区当天 00:00。 */
function startOfLocalDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

/**
 * 把时间范围换算成后端的 `since` / `until`（UTC RFC3339）。
 *
 * 两端都取本地零点而不是「现在减 N 天」：这样「今天」就是日历上的一天，跨时区、
 * 跨午夜都稳定；`until` 取明天零点（不含），保证同一段时间在会话内算出的查询条件不变，
 * 不会每次渲染都换一个新的游标查询。全部则不带时间条件。
 */
export function rangeBounds(
  range: TimeRange,
  now: Date,
): {
  since: string;
  until: string;
} | null {
  if (range === "all") {
    return null;
  }
  const days = range === "today" ? 0 : range === "7d" ? 6 : 29;
  const start = startOfLocalDay(now);
  start.setDate(start.getDate() - days);
  const until = startOfLocalDay(now);
  until.setDate(until.getDate() + 1);
  return { since: start.toISOString(), until: until.toISOString() };
}

/** URL 查询参数里的时间范围取值；无法识别时按「全部」处理。 */
export function parseRange(raw: string | null): TimeRange {
  const hit = TIME_RANGES.find((entry) => entry.value === raw);
  return hit?.value ?? "all";
}
