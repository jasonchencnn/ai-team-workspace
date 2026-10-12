import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router";

import {
  createTeam,
  inspectWorkspace,
  listRoleTemplates,
  pickWorkspaceFolder,
} from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import type { NewRole } from "../../bindings/NewRole.js";
import type { RoleTemplate } from "../../bindings/RoleTemplate.js";
import type { WorkspaceInfo } from "../../bindings/WorkspaceInfo.js";
import {
  Button,
  Card,
  ErrorBanner,
  InputField,
  PageHeader,
  Spinner,
  TextareaField,
} from "../../components/index.js";
import "./team-create.css";

/** 默认勾选的岗位模板（与设计稿 13:353 一致）。code 来自内核的 list_role_templates。 */
const DEFAULT_SELECTED = new Set(["role_leader", "role_frontend", "role_qa"]);

interface RoleDraft {
  enabled: boolean;
  name: string;
  taskPrefix: string;
  description: string;
}

function defaultDraft(template: RoleTemplate): RoleDraft {
  return {
    enabled: DEFAULT_SELECTED.has(template.code),
    name: template.name,
    taskPrefix: template.taskPrefix,
    description: template.description,
  };
}

/** 分支或提交缺失时如实说明，不编造。 */
function repoLine(workspace: WorkspaceInfo): string {
  const branch = workspace.branch ?? "detached HEAD";
  const commit = workspace.commit ?? "暂无提交";
  return `${branch} @ ${commit}`;
}

export function TeamCreatePage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const templates = useQuery({ queryKey: queryKeys.roleTemplates, queryFn: listRoleTemplates });

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [workspacePath, setWorkspacePath] = useState("");
  const [workspace, setWorkspace] = useState<WorkspaceInfo | null>(null);
  const [drafts, setDrafts] = useState<Record<string, RoleDraft>>({});
  const [submitted, setSubmitted] = useState(false);

  const draftOf = (template: RoleTemplate): RoleDraft =>
    drafts[template.code] ?? defaultDraft(template);

  const patchDraft = (template: RoleTemplate, patch: Partial<RoleDraft>) => {
    setDrafts((previous) => ({
      ...previous,
      [template.code]: { ...(previous[template.code] ?? defaultDraft(template)), ...patch },
    }));
  };

  const pick = useMutation({
    mutationFn: pickWorkspaceFolder,
    onSuccess: (info) => {
      // 用户取消对话框时后端返回 null，保持当前表单不变。
      if (info === null) {
        return;
      }
      setWorkspacePath(info.path);
      setWorkspace(info);
    },
  });

  const inspect = useMutation({
    mutationFn: inspectWorkspace,
    onSuccess: (info) => {
      setWorkspacePath(info.path);
      setWorkspace(info);
    },
  });

  const create = useMutation({
    mutationFn: createTeam,
    onSuccess: async (team) => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.teams });
      void navigate(`/teams/${team.id}/roles`);
    },
  });

  const draftList = templates.data ?? [];
  const nameError = submitted && name.trim() === "" ? "请填写 Team 名称" : undefined;

  const submit = () => {
    setSubmitted(true);
    const roles: NewRole[] = draftList
      .filter((template) => draftOf(template).enabled)
      .map((template) => {
        const draft = draftOf(template);
        return {
          name: draft.name,
          code: template.code,
          taskPrefix: draft.taskPrefix,
          description: draft.description,
          guidelines: template.guidelines,
          isLeader: template.isLeader,
        };
      });
    create.mutate({ name, description, workspacePath, roles });
  };

  return (
    <div className="tc-page">
      <PageHeader
        title="创建 Team"
        description="先建立协作边界，再创建稳定岗位并连接执行工具。"
        actions={
          <Button loading={create.isPending} onClick={submit}>
            创建团队并配置岗位
          </Button>
        }
      />

      <div className="tc-layout">
        <section className="tc-column" aria-labelledby="tc-team-heading">
          <h2 className="tc-column__title" id="tc-team-heading">
            1. 团队与工作空间
          </h2>

          <Card title={name.trim() === "" ? "未命名团队" : name} className="tc-stack-card">
            <div className="tc-stack">
              <InputField
                label="Team 名称"
                required
                value={name}
                error={nameError}
                placeholder="给团队起个名字"
                onChange={(event) => {
                  setName(event.currentTarget.value);
                }}
              />
              <TextareaField
                label="说明"
                value={description}
                placeholder="这个团队要推进什么"
                onChange={(event) => {
                  setDescription(event.currentTarget.value);
                }}
              />
              <p className="tc-note">团队状态：草稿 · 保存后创建</p>
            </div>
          </Card>

          <Card tone="info" title="本地 Git 工作空间" className="tc-stack-card">
            <div className="tc-stack">
              <p>
                <span className="nowrap">路径：</span>
                {workspacePath.trim() === "" ? "尚未选择工作空间" : workspacePath}
              </p>
              <p>共享工作区 + 任务执行工作区</p>
              <p>
                {workspace === null
                  ? "代码库状态：尚未检测"
                  : `代码库已检测：${repoLine(workspace)}`}
              </p>
            </div>
          </Card>

          <div className="tc-stack">
            <InputField
              label="手动输入工作空间路径"
              value={workspacePath}
              placeholder="/Users/you/projects/app"
              hint="填写后点「检测」按该路径重新检测 Git 仓库。"
              onChange={(event) => {
                setWorkspacePath(event.currentTarget.value);
                // 路径变了，之前的检测结果不再可信，回到未检测状态；校验态只由提交控制，不在这里重置。
                setWorkspace(null);
              }}
            />
            <div className="tc-actions">
              <Button
                variant="neutral"
                loading={pick.isPending}
                onClick={() => {
                  pick.mutate();
                }}
              >
                选择工作空间
              </Button>
              <Button
                variant="neutral"
                loading={inspect.isPending}
                disabled={workspacePath.trim() === ""}
                onClick={() => {
                  inspect.mutate(workspacePath.trim());
                }}
              >
                检测
              </Button>
            </div>
          </div>

          {pick.isError ? <ErrorBanner error={pick.error} /> : null}
          {inspect.isError ? <ErrorBanner error={inspect.error} /> : null}
        </section>

        <section className="tc-column" aria-labelledby="tc-roles-heading">
          <h2 className="tc-column__title" id="tc-roles-heading">
            2. 建立岗位（可在稍后调整）
          </h2>

          {templates.isPending ? <Spinner label="正在加载岗位模板" /> : null}
          {templates.isError ? (
            <ErrorBanner
              error={templates.error}
              action={
                <Button
                  variant="neutral"
                  onClick={() => {
                    void templates.refetch();
                  }}
                >
                  重试
                </Button>
              }
            />
          ) : null}

          {draftList.map((template) => {
            const draft = draftOf(template);
            return (
              <Card
                key={template.code}
                className="tc-stack-card tc-role"
                title={
                  <label className="tc-role__head">
                    <input
                      className="tc-role__checkbox"
                      type="checkbox"
                      checked={draft.enabled}
                      onChange={(event) => {
                        patchDraft(template, { enabled: event.currentTarget.checked });
                      }}
                    />
                    <span>{draft.name}</span>
                    <span className="tc-role__code">/ {template.code}</span>
                  </label>
                }
              >
                <div className="tc-stack">
                  {draft.enabled ? null : <p>{draft.description}</p>}
                  {draft.enabled ? (
                    <div className="tc-stack tc-role__fields">
                      <InputField
                        label="岗位名称"
                        value={draft.name}
                        onChange={(event) => {
                          patchDraft(template, { name: event.currentTarget.value });
                        }}
                      />
                      <InputField
                        label="任务前缀"
                        value={draft.taskPrefix}
                        hint="2–4 位大写字母，团队内唯一。"
                        onChange={(event) => {
                          patchDraft(template, { taskPrefix: event.currentTarget.value });
                        }}
                      />
                      <TextareaField
                        label="职责"
                        value={draft.description}
                        onChange={(event) => {
                          patchDraft(template, { description: event.currentTarget.value });
                        }}
                      />
                    </div>
                  ) : null}
                </div>
              </Card>
            );
          })}

          {templates.isSuccess && draftList.length === 0 ? (
            <p className="tc-note">内核没有返回岗位模板，可先创建团队再补岗位。</p>
          ) : null}
        </section>
      </div>

      {create.isError ? (
        <div className="tc-footer">
          <ErrorBanner
            error={create.error}
            action={
              <Button
                variant="neutral"
                onClick={() => {
                  create.reset();
                }}
              >
                知道了
              </Button>
            }
          />
        </div>
      ) : null}
    </div>
  );
}
