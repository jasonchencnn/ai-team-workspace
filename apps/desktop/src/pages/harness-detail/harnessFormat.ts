// Harness 类型、健康状态的中文标签与时间格式化：岗位页与 Harness 详情页共用同一套文案。
import type { HarnessKind } from "../../bindings/HarnessKind.js";
import type { HealthStatus } from "../../bindings/HealthStatus.js";
import type { TagTone } from "../../components/index.js";

const KIND_LABELS: Record<HarnessKind, string> = {
  claude_code: "Claude Code",
  codex: "Codex",
  mock: "Mock",
};

export function harnessKindLabel(kind: HarnessKind): string {
  return KIND_LABELS[kind];
}

const HEALTH_LABELS: Record<HealthStatus, string> = {
  ok: "在线",
  error: "异常",
  unknown: "未检查",
};

export function healthLabel(status: HealthStatus): string {
  return HEALTH_LABELS[status];
}

const HEALTH_TONES: Record<HealthStatus, TagTone> = {
  ok: "success",
  error: "warning",
  unknown: "neutral",
};

export function healthTone(status: HealthStatus): TagTone {
  return HEALTH_TONES[status];
}

function pad(value: number): string {
  return value < 10 ? `0${String(value)}` : String(value);
}

function parse(iso: string | null): Date | null {
  if (iso === null) {
    return null;
  }
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? null : date;
}

/** 2026-09-21 15:20（本地时区）；缺失或无法解析时返回占位符。 */
export function formatDateTime(iso: string | null): string {
  const date = parse(iso);
  if (date === null) {
    return "—";
  }
  return `${String(date.getFullYear())}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** 2026.09.18；缺失或无法解析时返回占位符。 */
export function formatDate(iso: string | null): string {
  const date = parse(iso);
  if (date === null) {
    return "—";
  }
  return `${String(date.getFullYear())}.${pad(date.getMonth() + 1)}.${pad(date.getDate())}`;
}

/** 「12 秒前」这类相对时间；超过一天回退为日期时间。`now` 由调用方传入，便于保持纯函数。 */
export function formatRelative(iso: string | null, now: number): string {
  const date = parse(iso);
  if (date === null) {
    return "未检查";
  }
  const seconds = Math.max(0, Math.round((now - date.getTime()) / 1000));
  if (seconds < 60) {
    return "刚刚";
  }
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return `${String(minutes)} 分钟前`;
  }
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return `${String(hours)} 小时前`;
  }
  return formatDateTime(iso);
}
