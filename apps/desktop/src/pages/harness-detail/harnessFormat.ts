// Harness 预设、握手状态的中文标签与时间格式化：岗位页与 Harness 详情页共用同一套文案。
// M1.3-03 只做了 ACP 契约的最小编译适配，页面由 M1.3-04 按 Figma 重做。
import type { AcpPreset } from "../../bindings/AcpPreset.js";
import type { Handshake } from "../../bindings/Handshake.js";
import type { HandshakeStatus } from "../../bindings/HandshakeStatus.js";
import type { TagTone } from "../../components/index.js";

const KIND_LABELS: Record<AcpPreset, string> = {
  claude_code: "Claude Code",
  codex: "Codex",
  minimax_code: "MiniMax Code",
  gemini_cli: "Gemini CLI",
  custom: "自定义 ACP Agent",
  mock: "Mock",
};

export function harnessKindLabel(kind: AcpPreset): string {
  return KIND_LABELS[kind];
}

/** Mock 是仅供验证的测试实例：卡片与绑定选择处都标「测试用」，避免误绑到正式岗位。 */
export function isTestHarness(kind: AcpPreset): boolean {
  return kind === "mock";
}

/** 测试用实例的标签文案。 */
export const TEST_HARNESS_LABEL = "测试用";

const HEALTH_LABELS: Record<HandshakeStatus, string> = {
  connected: "已连接",
  needs_auth: "需要登录",
  error: "异常",
  unknown: "未检查",
};

export function healthLabel(status: HandshakeStatus): string {
  return HEALTH_LABELS[status];
}

const HEALTH_TONES: Record<HandshakeStatus, TagTone> = {
  connected: "success",
  needs_auth: "warning",
  error: "warning",
  unknown: "neutral",
};

export function healthTone(status: HandshakeStatus): TagTone {
  return HEALTH_TONES[status];
}

/** 握手得到的 Agent 名称与版本，如 `claude-code-acp 0.9.2`；没有时为 null。 */
export function agentVersion(handshake: Handshake): string | null {
  const agent = handshake.agent;
  if (agent === null) {
    return null;
  }
  return agent.version === null ? agent.name : `${agent.name} ${agent.version}`;
}

/** 启动命令与参数拼成一行，供展示。 */
export function commandLine(command: string, args: string[]): string {
  return [command, ...args].join(" ");
}

/** 把「参数」输入框按空白拆成参数数组（最小适配，暂不支持带空格的参数）。 */
export function splitArgs(value: string): string[] {
  return value.split(/\s+/).filter((part) => part !== "");
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

/** 实例的 CLI 版本；尚未检测到时统一占位为「未检测」，卡片与详情页共用同一措辞。 */
export function formatVersion(version: string | null): string {
  return version ?? "未检测";
}

/** 「12 秒前」这类相对时间；从未检查过时返回「从未检查」，与状态标签「未检查」区分。 */
export function formatRelative(iso: string | null, now: number): string {
  const date = parse(iso);
  if (date === null) {
    return "从未检查";
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
