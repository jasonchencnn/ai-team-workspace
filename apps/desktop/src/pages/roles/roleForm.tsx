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

/**
 * 任务前缀自动建议：岗位名称填写后向内核要一个建议值。
 *
 * 返回的函数按 `key`（表单内岗位的唯一键）记录上次自动填入的前缀，只在该行的前缀为空、
 * 或仍是上次自动填入的值时才覆盖，用户手动改过的前缀不会被冲掉。
 * `teamId` 为空表示还在创建团队阶段（团队尚未存在），`taken` 传表单内其他岗位已填的前缀。
 */
export function useTaskPrefixSuggester(teamId: string | undefined, taken: string[]) {
  const autoFilled = useRef<Record<string, string>>({});

  const suggest = useMutation({
    mutationFn: (name: string) =>
      suggestTaskPrefix({
        teamId: teamId ?? null,
        name,
        taken: taken.filter((prefix) => prefix !== ""),
      }),
  });

  /** `apply` 由调用方提供，用来把建议值写回对应那一行的表单状态。 */
  return (key: string, name: string, currentPrefix: string, apply: (prefix: string) => void) => {
    if (name.trim() === "") {
      return;
    }
    suggest.mutate(name, {
      onSuccess: (suggested) => {
        const previousAuto = autoFilled.current[key];
        autoFilled.current[key] = suggested;
        if (currentPrefix !== "" && currentPrefix !== previousAuto) {
          // 用户手动改过这个前缀，尊重用户输入。
          return;
        }
        apply(suggested);
      },
    });
  };
}
