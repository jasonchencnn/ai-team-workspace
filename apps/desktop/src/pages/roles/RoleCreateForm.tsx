import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { createRole, listRoleTemplates } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import { Button, ErrorBanner, InputField, Panel, TextareaField } from "../../components/index.js";
import {
  EMPTY_ROLE_DRAFT,
  TASK_PREFIX_HINT,
  draftFromTemplate,
  roleNameError,
  useTaskPrefixSuggester,
} from "./roleForm.js";

export interface RoleCreateFormProps {
  teamId: string;
  onCancel: () => void;
  onCreated: () => void;
}

/**
 * 新增岗位：完全自定义（名称、前缀、职责、工作约定、是否 Leader）。
 * 岗位代码由后端自动生成，界面上不出现；「从示例添加」是可选的次要入口。
 */
export function RoleCreateForm({ teamId, onCancel, onCreated }: RoleCreateFormProps) {
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState(EMPTY_ROLE_DRAFT);
  const [showExamples, setShowExamples] = useState(false);
  const [submitted, setSubmitted] = useState(false);

  // 团队已存在，建议时会自动避开该团队已用的前缀。
  const suggestPrefix = useTaskPrefixSuggester(teamId, []);

  // 示例默认不加载，只在用户点开「从示例添加」时才取。
  const templatesQuery = useQuery({
    queryKey: queryKeys.roleTemplates,
    queryFn: listRoleTemplates,
    enabled: showExamples,
  });

  const createRoleMutation = useMutation({
    mutationFn: () =>
      createRole({
        teamId,
        role: {
          name: draft.name.trim(),
          taskPrefix: draft.taskPrefix.trim().toUpperCase(),
          description: draft.description,
          guidelines: draft.guidelines,
          isLeader: draft.isLeader,
        },
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.roles(teamId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.teamOverview(teamId) });
      onCreated();
    },
  });

  const update = <K extends keyof typeof draft>(key: K, value: (typeof draft)[K]) => {
    setDraft((previous) => ({ ...previous, [key]: value }));
  };

  const submit = () => {
    setSubmitted(true);
    if (draft.name.trim() === "") {
      return;
    }
    createRoleMutation.mutate();
  };

  const exampleList = templatesQuery.data ?? [];

  return (
    <Panel className="role-create">
      <h2 className="role-create__title">新增岗位</h2>
      <p className="role-create__note">岗位完全由你定义；下方示例只是预填快捷方式，可跳过。</p>

      <InputField
        label="岗位名称"
        required
        value={draft.name}
        error={roleNameError(draft.name, submitted)}
        placeholder="例如：资料整理"
        onChange={(event) => {
          update("name", event.target.value);
        }}
        onBlur={() => {
          suggestPrefix("new", draft.name, draft.taskPrefix, (prefix) => {
            update("taskPrefix", prefix);
          });
        }}
      />
      <InputField
        label="任务编号前缀"
        required
        hint={TASK_PREFIX_HINT}
        value={draft.taskPrefix}
        onChange={(event) => {
          update("taskPrefix", event.target.value);
        }}
      />
      <TextareaField
        label="职责"
        value={draft.description}
        placeholder="这个岗位负责什么"
        onChange={(event) => {
          update("description", event.target.value);
        }}
      />
      <TextareaField
        label="工作约定"
        value={draft.guidelines}
        placeholder="协作时希望遵守的约定"
        onChange={(event) => {
          update("guidelines", event.target.value);
        }}
      />
      <label className="role-create__leader">
        <input
          className="role-create__checkbox"
          type="checkbox"
          checked={draft.isLeader}
          onChange={(event) => {
            update("isLeader", event.target.checked);
          }}
        />
        <span>设为 Leader</span>
      </label>

      <div className="role-create__actions">
        <Button
          variant="subtle"
          onClick={() => {
            setShowExamples((previous) => !previous);
          }}
        >
          {showExamples ? "收起示例" : "从示例添加"}
        </Button>
      </div>

      {showExamples ? (
        <div className="role-create__examples">
          {templatesQuery.isPending ? <p className="role-create__note">正在加载示例…</p> : null}
          {templatesQuery.error === null ? null : <ErrorBanner error={templatesQuery.error} />}
          {!templatesQuery.isPending &&
          templatesQuery.error === null &&
          exampleList.length === 0 ? (
            <p className="role-create__note">没有可用的示例岗位，直接自定义即可。</p>
          ) : null}
          {exampleList.length > 0 ? (
            <ul className="role-create__example-list">
              {exampleList.map((template) => (
                <li key={template.code}>
                  <Button
                    variant="neutral"
                    onClick={() => {
                      setDraft(draftFromTemplate(template));
                    }}
                  >
                    {template.name}：{template.description}
                  </Button>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}

      {createRoleMutation.error === null ? null : <ErrorBanner error={createRoleMutation.error} />}

      <div className="role-create__actions">
        <Button
          loading={createRoleMutation.isPending}
          onClick={() => {
            submit();
          }}
        >
          创建岗位
        </Button>
        <Button variant="neutral" onClick={onCancel}>
          取消
        </Button>
      </div>
    </Panel>
  );
}
