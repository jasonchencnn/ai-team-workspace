// 看板分页的两处纯逻辑：把第一页与已加载的后续页拼起来，以及判断一列还有没有
// 没显示的任务。抽成纯函数是为了能单独测，不依赖 React 与内核。
import type { TaskSummary } from "../../bindings/TaskSummary.js";

/**
 * 把看板第一页与已经加载回来的后续页拼起来，按 `task.id` 去重。
 *
 * 看板数据刷新后（新建任务、状态变化、窗口聚焦重取）第一页会重新取一遍，
 * 而已经加载的后续页还留在手上。两边的排序基准变了就可能取到重叠的任务，
 * 直接拼起来会出现重复卡片。去重时以第一页的副本为准：它更新，状态也是新的。
 */
export function mergeBoardTasks(
  firstPage: readonly TaskSummary[],
  extra: readonly TaskSummary[],
): TaskSummary[] {
  const seen = new Set<string>();
  const merged: TaskSummary[] = [];
  for (const task of firstPage) {
    if (seen.has(task.id)) {
      continue;
    }
    seen.add(task.id);
    merged.push(task);
  }
  for (const task of extra) {
    if (seen.has(task.id)) {
      continue;
    }
    seen.add(task.id);
    merged.push(task);
  }
  return merged;
}

/**
 * 这一列还有没有没显示的任务。
 *
 * 两种情况都算「还有」：
 * - 后端还有没取回的（`hasMore`）；
 * - 本地已经取回但还没展开显示的（取回的条数 `pool` 多于显示的条数 `shown`）。
 *
 * 只看 `hasMore` 是不够的：后端翻完了但本地还有没展开的卡片时，按钮会消失，
 * 剩下的任务就再也看不到。
 */
export function columnHasHiddenTasks(hasMore: boolean, pool: number, shown: number): boolean {
  return hasMore || pool > shown;
}
