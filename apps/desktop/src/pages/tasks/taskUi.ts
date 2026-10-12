// 任务页面共用的展示辅助：状态到颜色的映射与时间格式。状态显示名一律来自后端。
import type { TagTone } from "../../components/index.js";
import type { TaskStatus } from "../../bindings/TaskStatus.js";

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
