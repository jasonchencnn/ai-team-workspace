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
  // `manual` 说明用户接管过这一行；当前值又与上次自动填入的不同，同样是用户自己的值。
  const userOwned = currentPrefix !== "" && (state.manual || currentPrefix !== state.autoFilled);
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

/** 一条回来的建议对某一行的处理结果。 */
export type SuggestionOutcome =
  /** 可以写入这一行（写入前仍由 `resolveAutoFillPrefix` 复核当前值）。 */
  | "apply"
  /** 岗位名称已经变了：这次建议是按旧名称算的，丢弃。 */
  | "stale-name"
  /** 用户接管过这一行：不再覆盖手填的前缀。 */
  | "manual";

/**
 * 判断一条回来的建议能不能写进这一行的前缀。纯函数，无副作用。
 *
 * 两道闸门各管一件事：
 * - 名称版本：建议是按**发出时**的名称算的，回来时岗位名称已经变了就丢弃；
 * - 用户接管：用户手动编辑过前缀（或正在前缀输入框里输入），迟到的建议不得覆盖。
 *
 * 前缀当前为空时视为用户把这一行交回自动填入，可以接受建议。
 */
export function checkSuggestion(
  requestedName: string,
  currentName: string,
  state: PrefixFillState,
  currentPrefix: string,
): SuggestionOutcome {
  if (currentName !== requestedName) {
    return "stale-name";
  }
  if (state.manual && currentPrefix !== "") {
    return "manual";
  }
  return "apply";
}

/**
 * 把一次建议写回某一行的表单状态。
 *
 * `resolve` 由调用方提供，必须包在**函数式 setState** 里：它拿到的是应用这次更新那一刻
 * 的前缀与岗位名称，而不是失焦那一刻的快照。命令式写值会读到过期快照，正是这个竞态的来源。
 */
export type ApplySuggestedPrefix = (
  resolve: (currentPrefix: string, currentName: string) => string,
) => void;

/** 向内核请求一次前缀建议。 */
export type SuggestPrefixRequest = (name: string) => Promise<string>;

/**
 * 一次前缀建议请求的票据：绑定**发出时**的岗位名称与该行当时的版本号。
 *
 * 每次调用各自持有一张票据，不再共享任何「在途回调」，因此返回顺序再乱，
 * 也不会出现旧请求借用新请求的判定逻辑。
 */
export interface PrefixSuggestionTicket {
  /** 表单内岗位的唯一键。 */
  key: string;
  /** 发出时的版本号；同一行的版本号单调递增。 */
  version: number;
  /** 发出时的岗位名称快照，**不**从提交时的闭包里再读名称。 */
  name: string;
}

/**
 * 按行记录版本号的闸门，回答「一条返回的建议是不是该行最新一次请求的结果」。
 * 按 `key` 分行：表单里其它岗位的请求不会互相作废。
 *
 * 版本号在两种情况下递增：发起一次新请求，以及用户在前缀输入框里动了手
 * （`touch`）——后者让这一行在途的建议立刻作废，迟到的建议不得写进用户正在编辑的输入框。
 */
export interface PrefixSuggestionGate {
  /** 为这一行发起一次新的建议，返回本次请求的票据。 */
  open(key: string, name: string): PrefixSuggestionTicket;
  /** 票据返回时，它是否仍是该行最新一次发起的请求。 */
  isCurrent(ticket: PrefixSuggestionTicket): boolean;
  /** 用户动过这一行的前缀输入框：该行在途的建议一律作废。 */
  touch(key: string): void;
}

export function createPrefixSuggestionGate(): PrefixSuggestionGate {
  const version: Record<string, number> = {};
  const bump = (key: string): number => {
    version[key] = (version[key] ?? 0) + 1;
    return version[key];
  };
  return {
    open(key, name) {
      return { key, version: bump(key), name };
    },
    isCurrent(ticket) {
      return version[ticket.key] === ticket.version;
    },
    touch(key) {
      bump(key);
    },
  };
}

/**
 * 发起一次建议请求，等它回来后**只有**在该票据仍是该行最新一次请求时才 `commit`。
 *
 * 判定与写入都发生在 `await` 之后，三道闸门各自负责一件事：
 * - 过期：同一行又失焦过（后发的请求版本号更大）→ 按旧名称算出的前缀直接丢弃，
 *   无论它是先回来还是后回来，前缀都停在最新名称的那次结果上；
 * - 用户动过前缀输入框：版本号被 `touch` 抬过 → 在途的建议作废，不会写进用户正在输入的框；
 * - 手填与名称版本：`commit` 收到建议后才去读那一行的最新前缀与名称
 *   （见 `checkSuggestion` 与 `resolveAutoFillPrefix`），等待期间用户手填或清空的值
 *   不会被迟到的建议冲掉。
 *
 * 请求失败时静默放弃这一次建议，表单保持用户当前的值。
 */
export async function runLatestSuggestion(
  gate: PrefixSuggestionGate,
  key: string,
  name: string,
  request: SuggestPrefixRequest,
  commit: (suggested: string, requestedName: string) => void,
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
    // 这一行已经按更新的名称重新问过内核（或用户动过前缀输入框），这次返回作废。
    return;
  }
  commit(suggested, ticket.name);
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
 * 按 `key`（表单内岗位的唯一键）记录该行的自动填入历史与请求版本号，
 * 并直接 `await` `suggestTaskPrefix`：每次调用持有自己的版本号与**发出时**的名称，
 * 返回时再核对「版本号是否仍是这一行最新一次」「岗位名称有没有变」「前缀有没有被用户
 * 手动改过」，三项都成立才写入。快速连续改名（Stage One → Stage Two → Final Delivery）
 * 只会采用最后一次名称的建议，不会被迟到的旧响应覆盖。
 *
 * 用户一旦在前缀输入框里动手（`touchPrefix`），这一行在途的建议立刻作废：迟到的建议
 * 不得写进用户正在编辑的输入框，也不得覆盖手填值（`setPrefixManual`）。
 *
 * `teamId` 为空表示还在创建团队阶段（团队尚未存在），`taken` 传表单内其他岗位已填的前缀。
 */
export interface TaskPrefixSuggester {
  /** 岗位名称失焦后向内核请求一次前缀建议。 */
  suggest(key: string, name: string, apply: ApplySuggestedPrefix): void;
  /** 用户在前缀输入框里动了手（聚焦或编辑）：这一行在途的建议一律作废。 */
  touchPrefix(key: string): void;
  /** 用户编辑了前缀：`manual` 为 true 表示接管这一行；清空（false）表示交回自动填入。 */
  setPrefixManual(key: string, manual: boolean): void;
}

export function useTaskPrefixSuggester(
  teamId: string | undefined,
  taken: string[],
): TaskPrefixSuggester {
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
  const suggest = (key: string, name: string, apply: ApplySuggestedPrefix) => {
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
      (suggested, requestedName) => {
        apply((currentPrefix, currentName) => {
          const state = fillState.current[key] ?? EMPTY_PREFIX_FILL;
          const outcome = checkSuggestion(requestedName, currentName, state, currentPrefix);
          if (outcome !== "apply") {
            // 名称已经变了（这次建议按旧名称算的），或用户接管了这一行：不写回。
            return currentPrefix;
          }
          const resolved = resolveAutoFillPrefix(state, currentPrefix, suggested);
          // 幂等记录：重放 updater 写入的是同一个值。
          pendingFill.current[key] = resolved.state;
          return resolved.prefix;
        });
      },
    );
  };

  const touchPrefix = (key: string) => {
    // 用户动了前缀输入框（聚焦或编辑）：这一行在途的建议作废，迟到的建议不得写进来。
    gate.touch(key);
  };

  const setPrefixManual = (key: string, manual: boolean) => {
    const state = fillState.current[key] ?? EMPTY_PREFIX_FILL;
    const next = { ...state, manual };
    fillState.current[key] = next;
    // 待落盘的记录一起改，否则它会在下一次渲染后把 manual 改回去。
    pendingFill.current[key] = next;
  };

  return { suggest, touchPrefix, setPrefixManual };
}
