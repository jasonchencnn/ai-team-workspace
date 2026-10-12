import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { createRole, listRoleTemplates } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  ErrorBanner,
  InputField,
  Panel,
  SelectField,
  TextareaField,
} from "../../components/index.js";

export interface RoleCreateFormProps {
  teamId: string;
  onCancel: () => void;
  onCreated: () => void;
}

interface RoleDraft {
  name: string;
  code: string;
  taskPrefix: string;
  description: string;
  guidelines: string;
  isLeader: boolean;
}

const EMPTY_DRAFT: RoleDraft = {
  name: "",
  code: "",
  taskPrefix: "",
  description: "",
  guidelines: "",
  isLeader: false,
};

const CUSTOM_TEMPLATE = "custom";

/** 新增岗位：可选内置模板预填，也可完全自定义。 */
export function RoleCreateForm({ teamId, onCancel, onCreated }: RoleCreateFormProps) {
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState<RoleDraft>(EMPTY_DRAFT);
  const [templateCode, setTemplateCode] = useState<string>(CUSTOM_TEMPLATE);

  const templatesQuery = useQuery({
    queryKey: queryKeys.roleTemplates,
    queryFn: listRoleTemplates,
  });

  const createRoleMutation = useMutation({
    mutationFn: () => createRole({ teamId, role: draft }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.roles(teamId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.teamOverview(teamId) });
      onCreated();
    },
  });

  const update = <K extends keyof RoleDraft>(key: K, value: RoleDraft[K]) => {
    setDraft((previous) => ({ ...previous, [key]: value }));
  };

  return (
    <Panel className="role-create">
      <h2 className="role-create__title">新增岗位</h2>

      {templatesQuery.error === null ? null : <ErrorBanner error={templatesQuery.error} />}

      <SelectField
        label="岗位模板"
        hint="选择模板可预填名称、编号前缀和职责；之后仍可修改。"
        value={templateCode}
        onChange={(event) => {
          const code = event.target.value;
          setTemplateCode(code);
          const template = templatesQuery.data?.find((item) => item.code === code);
          if (template === undefined) {
            setDraft(EMPTY_DRAFT);
            return;
          }
          setDraft({
            name: template.name,
            code: template.code,
            taskPrefix: template.taskPrefix,
            description: template.description,
            guidelines: template.guidelines,
            isLeader: template.isLeader,
          });
        }}
      >
        <option value={CUSTOM_TEMPLATE}>自定义岗位</option>
        {templatesQuery.data?.map((template) => (
          <option key={template.code} value={template.code}>
            {template.name}
          </option>
        ))}
      </SelectField>

      <InputField
        label="岗位名称"
        required
        value={draft.name}
        onChange={(event) => {
          update("name", event.target.value);
        }}
      />
      <InputField
        label="岗位标识"
        required
        hint="小写字母开头，只含小写字母、数字和下划线；创建后不可修改。"
        value={draft.code}
        onChange={(event) => {
          update("code", event.target.value);
        }}
      />
      <InputField
        label="编号前缀"
        required
        hint="2–4 位大写字母，团队内唯一。"
        value={draft.taskPrefix}
        onChange={(event) => {
          update("taskPrefix", event.target.value);
        }}
      />
      <TextareaField
        label="职责"
        value={draft.description}
        onChange={(event) => {
          update("description", event.target.value);
        }}
      />
      <TextareaField
        label="工作约定"
        value={draft.guidelines}
        onChange={(event) => {
          update("guidelines", event.target.value);
        }}
      />

      {createRoleMutation.error === null ? null : <ErrorBanner error={createRoleMutation.error} />}

      <div className="role-create__actions">
        <Button
          loading={createRoleMutation.isPending}
          onClick={() => {
            createRoleMutation.mutate();
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
