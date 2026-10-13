import { useMutation } from "@tanstack/react-query";
import { useRef } from "react";

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

/**
 * 任务前缀自动建议：岗位名称失焦后向内核要一个建议值。
 *
 * 返回的函数按 `key`（表单内岗位的唯一键）记录该行的自动填入历史与在途请求序号：
 * - 建议到达时用最新前缀值做判定（见 `resolveAutoFillPrefix`），手填值不会被冲掉；
 * - 同一行又发起过更新的请求时，序号不匹配的返回结果按过期丢弃。
 *
 * `teamId` 为空表示还在创建团队阶段（团队尚未存在），`taken` 传表单内其他岗位已填的前缀。
 */
export function useTaskPrefixSuggester(teamId: string | undefined, taken: string[]) {
  const fillState = useRef<Record<string, PrefixFillState>>({});
  const requestSeq = useRef<Record<string, number>>({});

  const suggest = useMutation({
    mutationFn: (name: string) =>
      suggestTaskPrefix({
        teamId: teamId ?? null,
        name,
        taken: taken.filter((prefix) => prefix !== ""),
      }),
  });

  /** `apply` 由调用方提供，用来把判定后的前缀写回对应那一行的表单状态。 */
  return (key: string, name: string, apply: ApplySuggestedPrefix) => {
    if (name.trim() === "") {
      return;
    }
    const seq = (requestSeq.current[key] ?? 0) + 1;
    requestSeq.current[key] = seq;
    suggest.mutate(name, {
      onSuccess: (suggested) => {
        if (requestSeq.current[key] !== seq) {
          // 名称又变了，这次建议已经过期，丢弃，避免用旧名称的结果去覆盖。
          return;
        }
        apply((currentPrefix) => {
          const state = fillState.current[key] ?? EMPTY_PREFIX_FILL;
          const resolved = resolveAutoFillPrefix(state, currentPrefix, suggested);
          fillState.current[key] = resolved.state;
          return resolved.prefix;
        });
      },
    });
  };
}
