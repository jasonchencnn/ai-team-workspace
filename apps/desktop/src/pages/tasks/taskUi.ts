// 任务页面共用的展示辅助：状态到颜色的映射与时间格式。状态显示名一律来自后端。
import type { TagTone } from "../../components/index.js";
import type { TaskStatus } from "../../bindings/TaskStatus.js";

export type ViewMode = "board" | "list";
export type FirstColumnFilter = "all" | "todo" | "ready";

/** 面板上的视图与筛选。默认值不写进地址栏，链接保持干净。 */
export interface TaskPanelState {
  view: ViewMode;
  filter: FirstColumnFilter;
  showDone: boolean;
  showCancelled: boolean;
}

export const VIEW_PARAM = "view";
export const FILTER_PARAM = "filter";
export const DONE_PARAM = "done";
export const CANCELLED_PARAM = "cancelled";

export const DEFAULT_PANEL: TaskPanelState = {
  view: "board",
  filter: "all",
  showDone: false,
  showCancelled: false,
};

const FIRST_COLUMN_FILTERS: readonly FirstColumnFilter[] = ["all", "todo", "ready"];

/**
 * 从地址栏读出面板状态。视图与筛选跟活动与审计页一样放在 URL 上，所以刷新、复制链接、
 * 从任务详情返回都能保持一致；无法识别的取值按默认值处理，不在这里报错。
 */
export function parseTaskPanel(params: URLSearchParams): TaskPanelState {
  const filter = params.get(FILTER_PARAM);
  return {
    view: params.get(VIEW_PARAM) === "list" ? "list" : "board",
    filter: FIRST_COLUMN_FILTERS.find((value) => value === filter) ?? DEFAULT_PANEL.filter,
    // 勾选框只写 1；别的写法（包括拼错的）当没勾选。
    showDone: params.get(DONE_PARAM) === "1",
    showCancelled: params.get(CANCELLED_PARAM) === "1",
  };
}

/** 把面板状态写回地址栏；与默认值相同的项不写，顺序固定，方便对照和复现。 */
export function serializeTaskPanel(state: TaskPanelState): URLSearchParams {
  const params = new URLSearchParams();
  if (state.view !== DEFAULT_PANEL.view) {
    params.set(VIEW_PARAM, state.view);
  }
  if (state.filter !== DEFAULT_PANEL.filter) {
    params.set(FILTER_PARAM, state.filter);
  }
  if (state.showDone) {
    params.set(DONE_PARAM, "1");
  }
  if (state.showCancelled) {
    params.set(CANCELLED_PARAM, "1");
  }
  return params;
}

/**
 * 从任意来源的查询串里取出属于任务面板的那部分（默认值不带），拼成可直接接在面板路径后的
 * 搜索串。任务详情是从面板、活动页等多处进来的，只有面板自己的参数值得带回去，
 * 别的页面的筛选参数不该混进面板地址栏。
 */
export function taskPanelSearch(search: string): string {
  const text = serializeTaskPanel(parseTaskPanel(new URLSearchParams(search))).toString();
  return text === "" ? "" : `?${text}`;
}

export const statusTone: Record<TaskStatus, TagTone> = {
  todo: "neutral",
  ready: "info",
  doing: "info",
  blocked: "warning",
  review: "success",
  done: "success",
  cancelled: "neutral",
};

/** 把后端的 UTC 时间转成本地时区的可读时间；无法解析时原样返回。 */
export function formatTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return iso;
  }
  return date.toLocaleString("zh-CN", { hour12: false });
}
