import { useEffect, useRef } from "react";

import { suggestTaskPrefix } from "../../api/commands.js";
import type { RoleTemplate } from "../../bindings/RoleTemplate.js";

/** 岗位表单的字段集合；岗位代码由后端自动生成，界面上不出现。 */
export interface RoleDraft {
  name: string;
  taskPrefix: string;
  description: string;
  guidelines: string;
  isLeader: boolean;
}

export const EMPTY_ROLE_DRAFT: RoleDraft = {
  name: "",
  taskPrefix: "",
  description: "",
  guidelines: "",
  isLeader: false,
};

/** 由内置示例预填出的岗位草稿，用户可任意修改。 */
export function draftFromTemplate(template: RoleTemplate): RoleDraft {
  return {
    name: template.name,
    taskPrefix: template.taskPrefix,
    description: template.description,
    guidelines: template.guidelines,
    isLeader: template.isLeader,
  };
}

/** 岗位名称必填；提交后为空才提示，避免边输入边报错。 */
export function roleNameError(name: string, submitted: boolean): string | undefined {
  return submitted && name.trim() === "" ? "请填写岗位名称" : undefined;
}

export const TASK_PREFIX_HINT = "2–4 位大写字母或数字，以字母开头；填写名称后自动建议，可修改。";

/** 表单内某一行岗位的前缀自动填入状态。 */
export interface PrefixFillState {
  /** 上一次自动写入的前缀；`null` 表示从未自动填入过。 */
  autoFilled: string | null;
  /** 用户是否手动改过这一行的前缀。 */
  manual: boolean;
}

export const EMPTY_PREFIX_FILL: PrefixFillState = { autoFilled: null, manual: false };

/** 一次自动填入的判定结果。 */
export interface PrefixFillResult {
  /** 这一行接下来应持有的前缀。 */
  prefix: string;
  /** 前缀是否真的被改写了；`false` 表示保留用户手填的值。 */
  changed: boolean;
  /** 判定后的行状态，供下一次建议沿用。 */
  state: PrefixFillState;
}

/**
 * 判断一条到达的建议能否写入这一行的前缀，并给出判定后的行状态。纯函数，无副作用。
 *
 * 只有前缀**当前**仍为空、或仍等于上一次自动填入的值（用户没手动改过）时才写入。
 * `currentPrefix` 必须传建议返回那一刻的最新值：异步回来后还要做一次这个判定，
 * 否则用户在等待期间手填的前缀会被迟到的建议冲掉。
 * 用户手动改过之后该行记为 `manual`，后续名称变化不再自动覆盖；但用户自己清空前缀
 * 等于把这一行交回自动填入，会重新接受建议。
 *
 * 重复应用是幂等的：同一状态下再算一次不会改变结果，React 重放 updater 也不会跑偏。
 */
export function resolveAutoFillPrefix(
  state: PrefixFillState,
  currentPrefix: string,
  suggested: string,
): PrefixFillResult {
  const userOwned = currentPrefix !== "" && currentPrefix !== state.autoFilled;
  if (userOwned) {
    // 用户手动改过这个前缀（或曾经改过），尊重用户输入，不覆盖。
    return {
      prefix: currentPrefix,
      changed: false,
      state: { autoFilled: state.autoFilled, manual: true },
    };
  }
  return {
    prefix: suggested,
    changed: suggested !== currentPrefix,
    state: { autoFilled: suggested, manual: false },
  };
}

/**
 * 把一次建议写回某一行的表单状态。
 *
 * `resolve` 由调用方提供，必须包在**函数式 setState** 里：它拿到的是应用这次更新那一刻
 * 的前缀值，而不是失焦那一刻的快照。命令式写值会读到过期快照，正是这个竞态的来源。
 */
export type ApplySuggestedPrefix = (resolve: (currentPrefix: string) => string) => void;

/** 向内核请求一次前缀建议。 */
export type SuggestPrefixRequest = (name: string) => Promise<string>;

/**
 * 一次前缀建议请求的票据：绑定**发出时**的岗位名称与该行当时的请求序号。
 *
 * 每次调用各自持有一张票据，不再共享任何「在途回调」，因此返回顺序再乱，
 * 也不会出现旧请求借用新请求的判定逻辑。
 */
export interface PrefixSuggestionTicket {
  /** 表单内岗位的唯一键。 */
  key: string;
  /** 发出时的请求序号；同一行的序号单调递增。 */
  seq: number;
  /** 发出时的岗位名称快照，**不**从提交时的闭包里再读名称。 */
  name: string;
}

/**
 * 按行记录在途请求序号的闸门，回答「一条返回的建议是不是该行最新一次请求的结果」。
 * 按 `key` 分行：表单里其它岗位的请求不会互相作废。
 */
export interface PrefixSuggestionGate {
  /** 为这一行发起一次新的建议，返回本次请求的票据。 */
  open(key: string, name: string): PrefixSuggestionTicket;
  /** 票据返回时，它是否仍是该行最新一次发起的请求。 */
  isCurrent(ticket: PrefixSuggestionTicket): boolean;
}

export function createPrefixSuggestionGate(): PrefixSuggestionGate {
  const latestSeq: Record<string, number> = {};
  return {
    open(key, name) {
      const seq = (latestSeq[key] ?? 0) + 1;
      latestSeq[key] = seq;
      return { key, seq, name };
    },
    isCurrent(ticket) {
      return latestSeq[ticket.key] === ticket.seq;
    },
  };
}

/**
 * 发起一次建议请求，等它回来后**只有**在该票据仍是该行最新一次请求时才 `commit`。
 *
 * 判定与写入都发生在 `await` 之后，两道闸门各自负责一件事：
 * - 过期：同一行又失焦过（后发的请求序号更大）→ 按旧名称算出的前缀直接丢弃，
 *   无论它是先回来还是后回来，前缀都停在最新名称的那次结果上；
 * - 手填：`commit` 收到建议后才去读那一行的最新前缀（见 `resolveAutoFillPrefix`），
 *   等待期间用户手填或清空的值不会被迟到的建议冲掉。
 *
 * 请求失败时静默放弃这一次建议，表单保持用户当前的值。
 */
export async function runLatestSuggestion(
  gate: PrefixSuggestionGate,
  key: string,
  name: string,
  request: SuggestPrefixRequest,
  commit: (suggested: string) => void,
): Promise<void> {
  const ticket = gate.open(key, name);
  let suggested: string;
  try {
    suggested = await request(ticket.name);
  } catch {
    // 建议是锦上添花，取不到就保持前缀原样，不打扰用户。
    return;
  }
  if (!gate.isCurrent(ticket)) {
    // 这一行已经按更新的名称重新问过内核，这次返回对应的是旧名称，丢弃。
    return;
  }
  commit(suggested);
}

/**
 * 把渲染阶段记下的行状态落盘。纯函数，方便直接断言。
 *
 * 判定结果**不能**在 `setState` 的 updater 里直接写进 `fillState`：React 会重放 updater
 * （StrictMode 下双调用），第二次重放会读到第一次刚写入的 `autoFilled`，于是把本行
 * 上一次自动写入的值误判成「用户手动填写」而拒绝覆盖。updater 里只做幂等的记录，
 * 由 `commitPendingFillStates` 在渲染之外统一落盘；调用方随后丢弃 pending 记录本身。
 */
export function commitPendingFillStates(
  fillState: Record<string, PrefixFillState>,
  pending: Record<string, PrefixFillState>,
): void {
  for (const [key, state] of Object.entries(pending)) {
    fillState[key] = state;
  }
}

/**
 * 任务前缀自动建议：岗位名称失焦后向内核要一个建议值。
 *
 * 返回的函数按 `key`（表单内岗位的唯一键）记录该行的自动填入历史与请求序号，
 * 并直接 `await` `suggestTaskPrefix`：每次调用持有自己的序号与**发出时**的名称，
 * 返回时再核对「序号是否仍是这一行最新一次」和「前缀有没有被用户手动改过」，
 * 两项都成立才写入。快速连续改名（Stage One → Stage Two → Final Delivery）
 * 只会采用最后一次名称的建议，不会被迟到的旧响应覆盖。
 *
 * `teamId` 为空表示还在创建团队阶段（团队尚未存在），`taken` 传表单内其他岗位已填的前缀。
 */
export function useTaskPrefixSuggester(teamId: string | undefined, taken: string[]) {
  const fillState = useRef<Record<string, PrefixFillState>>({});
  const pendingFill = useRef<Record<string, PrefixFillState>>({});
  const gate = useRef(createPrefixSuggestionGate()).current;

  // 判定后的行状态在渲染之外落盘：写在 updater 里会被 React 重放，重放读到的是自己
  // 上一次写入的 autoFilled，会把自动值误判成用户手填（见 commitPendingFillStates）。
  useEffect(() => {
    commitPendingFillStates(fillState.current, pendingFill.current);
    pendingFill.current = {};
  });

  /** `apply` 由调用方提供，用来把判定后的前缀写回对应那一行的表单状态。 */
  return (key: string, name: string, apply: ApplySuggestedPrefix) => {
    if (name.trim() === "") {
      return;
    }
    void runLatestSuggestion(
      gate,
      key,
      name,
      (requestedName) =>
        suggestTaskPrefix({
          teamId: teamId ?? null,
          name: requestedName,
          taken: taken.filter((prefix) => prefix !== ""),
        }),
      (suggested) => {
        apply((currentPrefix) => {
          const state = fillState.current[key] ?? EMPTY_PREFIX_FILL;
          const resolved = resolveAutoFillPrefix(state, currentPrefix, suggested);
          // 幂等记录：重放 updater 写入的是同一个值。
          pendingFill.current[key] = resolved.state;
          return resolved.prefix;
        });
      },
    );
  };
}
