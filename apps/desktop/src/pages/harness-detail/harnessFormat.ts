// Harness 预设、握手状态的中文标签与时间格式化：岗位工作台、岗位页与接入详情页共用同一套文案。
import type { AcpCapabilities } from "../../bindings/AcpCapabilities.js";
import type { AcpPreset } from "../../bindings/AcpPreset.js";
import type { Handshake } from "../../bindings/Handshake.js";
import type { HandshakeStatus } from "../../bindings/HandshakeStatus.js";
import type { TagTone } from "../../components/index.js";

const KIND_LABELS: Record<AcpPreset, string> = {
  claude_code: "Claude Code",
  codex: "Codex",
  minimax_code: "MiniMax Code",
  gemini_cli: "Gemini CLI",
  codebuddy: "CodeBuddy（腾讯）",
  qoder_cli: "Qoder CLI（阿里）",
  grok_build: "Grok Build（xAI）",
  qwen_code: "Qwen Code（阿里）",
  kimi_cli: "Kimi CLI（月之暗面）",
  opencode: "OpenCode",
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

/**
 * 预设标签：岗位工作台与接入列表上的胶囊文案，如 `Claude Code · ACP`。
 * 真实预设说明它们是经适配器接入 ACP 的 Agent；自定义与 Mock 单独措辞。
 */
export function presetTagLabel(kind: AcpPreset): string {
  switch (kind) {
    case "claude_code":
      return "Claude Code · ACP";
    case "codex":
      return "Codex · ACP";
    case "minimax_code":
      return "MiniMax Code · ACP";
    case "gemini_cli":
      return "Gemini CLI · ACP";
    case "codebuddy":
      return "CodeBuddy · ACP";
    case "qoder_cli":
      return "Qoder CLI · ACP";
    case "grok_build":
      return "Grok Build · ACP";
    case "qwen_code":
      return "Qwen Code · ACP";
    case "kimi_cli":
      return "Kimi CLI · ACP";
    case "opencode":
      return "OpenCode · ACP";
    case "custom":
      return "自定义 ACP Agent";
    case "mock":
      return `Mock ${TEST_HARNESS_LABEL}`;
  }
}

/**
 * 握手声明的能力标签（如「加载会话」「图片输入」「MCP」）。
 * 未握手成功时能力为 null，此时没有标签可展示。
 */
export function capabilityLabels(capabilities: AcpCapabilities | null): string[] {
  if (capabilities === null) {
    return [];
  }
  const labels: string[] = [];
  if (capabilities.loadSession) {
    labels.push("加载会话");
  }
  if (capabilities.image) {
    labels.push("图片输入");
  }
  if (capabilities.audio) {
    labels.push("音频输入");
  }
  if (capabilities.embeddedContext) {
    labels.push("嵌入上下文");
  }
  if (capabilities.mcpHttp) {
    labels.push("MCP(HTTP)");
  }
  if (capabilities.mcpSse) {
    labels.push("MCP(SSE)");
  }
  return labels;
}

/** ACP 协议版本的展示文案，如 `ACP v1`；未握手成功时为 null。 */
export function protocolVersionLabel(handshake: Handshake): string | null {
  return handshake.protocolVersion === null ? null : `ACP v${String(handshake.protocolVersion)}`;
}

/**
 * 握手状态的补充说明：已连接时说明可以派工，需要登录时提示去对应工具登录，
 * 异常时说明去接入列表看错误，未检查时说明尚未做过握手检查。
 */
export function handshakeHint(status: HandshakeStatus): string {
  switch (status) {
    case "connected":
      return "握手成功，可以派工。";
    case "needs_auth":
      return "Agent 要求登录，请在对应工具中完成登录后重新检查。";
    case "error":
      return "启动或握手失败，查看接入列表中的错误说明。";
    case "unknown":
      return "尚未做过握手检查。";
  }
}

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
  error: "danger",
  unknown: "neutral",
};

export function healthTone(status: HandshakeStatus): TagTone {
  return HEALTH_TONES[status];
}

/**
 * 握手说明行的语气：已连接与未检查是常规信息，用灰色；
 * 需要登录才用警告色，启动失败等异常用危险色。
 * 避免「握手成功」这类正常说明被渲染成警告，看起来像出了问题。
 */
export function handshakeDetailTone(status: HandshakeStatus): "muted" | "warning" | "danger" {
  switch (status) {
    case "needs_auth":
      return "warning";
    case "error":
      return "danger";
    case "connected":
    case "unknown":
      return "muted";
  }
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
