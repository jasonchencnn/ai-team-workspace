// 换绑（Harness 重新绑定）的纯逻辑：方式文案、候选实例筛选、待生效摘要与待投递消息判定。
//
// 怎么换、何时换由内核决定（crates/teamspace-core/src/service/rebind.rs），
// 前端只负责把后端返回的事实翻译成中文文案；所有判定都写成纯函数，单元测试直接锁住。
import type { HarnessInstance } from "../../bindings/HarnessInstance.js";
import type { PendingBinding } from "../../bindings/PendingBinding.js";
import type { RebindMode } from "../../bindings/RebindMode.js";
import type { RunStatus } from "../../bindings/RunStatus.js";

/** 三种换绑方式，与内核的 RebindMode 一一对应。 */
export const REBIND_MODES: RebindMode[] = ["after_current", "cancel_and_rerun", "manual_takeover"];

/**
 * 默认方式：当前执行完成后更换。
 * 与 Figma 6:403 的推荐项一致，也是内核在没有进行中执行时的行为。
 */
export const DEFAULT_REBIND_MODE: RebindMode = "after_current";

export interface RebindModeOption {
  mode: RebindMode;
  /** 选项文案。 */
  label: string;
  /** 选它之后会发生什么。 */
  consequence: string;
  recommended: boolean;
}

/**
 * 换绑方式选项。
 * 「推荐」只标在当前执行完成后更换上：它不打断正在跑的活，是唯一无损的做法。
 */
export const REBIND_MODE_OPTIONS: RebindModeOption[] = [
  {
    mode: "after_current",
    label: "当前任务完成后更换",
    consequence: "停止领取新任务，当前执行可以继续；新绑定在当前执行结束后生效。",
    recommended: true,
  },
  {
    mode: "cancel_and_rerun",
    label: "取消当前执行，由新 Harness 重新执行",
    consequence: "取消当前执行并立即换绑；该任务会在新 Harness 上重新开始执行。",
    recommended: false,
  },
  {
    mode: "manual_takeover",
    label: "立即人工接管，暂不自动执行",
    consequence: "取消当前执行并立即换绑；任务转为受阻（人工接管），由你接手处理。",
    recommended: false,
  },
];

const MODE_LABELS: Record<RebindMode, string> = {
  after_current: "当前执行完成后更换",
  cancel_and_rerun: "取消并重跑",
  manual_takeover: "人工接管",
};

/** 方式的中文短名，用在「待生效」摘要里。 */
export function rebindModeLabel(mode: RebindMode): string {
  return MODE_LABELS[mode];
}

/** 取某个方式的完整选项文案；未知方式退到默认项，避免界面出现空标签。 */
export function rebindModeOption(mode: RebindMode): RebindModeOption {
  return (
    REBIND_MODE_OPTIONS.find((option) => option.mode === mode) ??
    (REBIND_MODE_OPTIONS[0] as RebindModeOption)
  );
}

/**
 * 仍算「进行中」的执行状态。
 * 与内核的 ACTIVE_SQL（queued / running / waiting_permission）保持一致：
 * 只有这三种执行还在跑，换绑才有「完成后更换」这个选项。
 */
export const ACTIVE_RUN_STATUSES: RunStatus[] = ["queued", "running", "waiting_permission"];

/** 判断一个执行状态是否仍在进行中。 */
export function isActiveRunStatus(status: RunStatus): boolean {
  return ACTIVE_RUN_STATUSES.includes(status);
}

/**
 * 换绑可选的目标实例：同团队、非归档，且不是当前已绑的实例。
 * 当前实例不列出来，免得用户选了一个等于没换。
 */
export function rebindCandidates(
  harnesses: HarnessInstance[],
  currentHarnessId: string | null,
): HarnessInstance[] {
  return harnesses.filter(
    (harness) => harness.id !== currentHarnessId && harness.archivedAt === null,
  );
}

export interface PendingBindingSummary {
  /** 「待生效：旧 → 新」这一行。 */
  title: string;
  /** 括号里的说明：方式与发起时间。 */
  detail: string;
  oldName: string;
  newName: string;
  modeLabel: string;
  requestedAt: string;
}

/**
 * 待生效绑定摘要。
 * 旧实例名来自岗位的当前绑定（pendingBinding 只记目标实例）；
 * 岗位没绑实例时显示「未绑定」，不编造名字。
 */
export function pendingBindingSummary(
  pending: PendingBinding,
  currentHarnessName: string | null,
  requestedAtText: string,
): PendingBindingSummary {
  const oldName = currentHarnessName ?? "未绑定";
  const newName = pending.harnessName;
  const modeLabel = rebindModeLabel(pending.mode);
  return {
    title: `待生效：${oldName} → ${newName}`,
    detail: `（${modeLabel}，${requestedAtText}）`,
    oldName,
    newName,
    modeLabel,
    requestedAt: requestedAtText,
  };
}

/**
 * 待投递消息的说明行。
 * bindingName 是消息入队那一刻的绑定快照；换绑生效后这些消息会按新绑定投递。
 */
export function pendingDeliveryNote(bindingName: string | null): string {
  return `入队时绑定：${bindingName ?? "无有效绑定"}，将按生效后的绑定投递`;
}

/** 启动包预览为空时的提示：已执行过就不需要交接。 */
export const HANDOFF_EMPTY_TEXT = "当前 Harness 已在此岗位执行过，不会附加交接";

/** 岗位没有绑定实例时的启动包提示：没东西可交接。 */
export const HANDOFF_EMPTY_TEXT_UNBOUND = "岗位未绑定 Harness，不会附加交接";

/** 按岗位是否有绑定实例，选启动包为空时的提示文案。 */
export function handoffEmptyText(hasHarness: boolean): string {
  return hasHarness ? HANDOFF_EMPTY_TEXT : HANDOFF_EMPTY_TEXT_UNBOUND;
}
