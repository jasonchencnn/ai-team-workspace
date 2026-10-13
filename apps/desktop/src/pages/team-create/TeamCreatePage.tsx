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
  EmptyState,
  ErrorBanner,
  InputField,
  PageHeader,
  TextareaField,
} from "../../components/index.js";
import {
  EMPTY_ROLE_DRAFT,
  TASK_PREFIX_HINT,
  draftFromTemplate,
  roleNameError,
  useTaskPrefixSuggester,
} from "../roles/roleForm.js";
import type { RoleDraft } from "../roles/roleForm.js";
import "./team-create.css";

/** 表单内每个岗位卡的临时键；岗位代码由后端生成，这里只用它区分多行输入。 */
type DraftKey = string;

interface DraftEntry {
  key: DraftKey;
  draft: RoleDraft;
}

let draftSequence = 0;
const nextKey = (): DraftKey => {
  draftSequence += 1;
  return `draft-${String(draftSequence)}`;
};

/** 工作空间类型说明：Git 仓库显示分支与提交，普通文件夹如实说明，不编造分支。 */
function workspaceLine(workspace: WorkspaceInfo): string {
  if (workspace.git === null) {
    return "普通文件夹";
  }
  const branch = workspace.git.branch ?? "detached HEAD";
  const commit = workspace.git.commit ?? "暂无提交";
  return `Git 仓库：${branch} @ ${commit}`;
}

export function TeamCreatePage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [workspacePath, setWorkspacePath] = useState("");
  const [workspace, setWorkspace] = useState<WorkspaceInfo | null>(null);

  const [entries, setEntries] = useState<DraftEntry[]>([]);
  const [showExamples, setShowExamples] = useState(false);
  const [submitted, setSubmitted] = useState(false);

  // 团队尚未创建，teamId 传空；已填前缀一起传出去，建议值不会与表单内其它岗位撞号。
  const suggestPrefix = useTaskPrefixSuggester(
    undefined,
    entries.map((entry) => entry.draft.taskPrefix),
  );

  // 示例默认不加载：只有用户点开「从示例添加」才去取 list_role_templates。
  const templates = useQuery({
    queryKey: queryKeys.roleTemplates,
    queryFn: listRoleTemplates,
    enabled: showExamples,
  });

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
      void navigate(`/teams/${team.id}`);
    },
  });

  const addDraft = (draft: RoleDraft = EMPTY_ROLE_DRAFT) => {
    setEntries((previous) => [...previous, { key: nextKey(), draft }]);
  };

  const removeDraft = (key: DraftKey) => {
    setEntries((previous) => previous.filter((entry) => entry.key !== key));
  };

  const patchDraft = (key: DraftKey, patch: Partial<RoleDraft>) => {
    setEntries((previous) =>
      previous.map((entry) =>
        entry.key === key ? { ...entry, draft: { ...entry.draft, ...patch } } : entry,
      ),
    );
  };

  const submit = () => {
    setSubmitted(true);
    const roles: NewRole[] = entries
      .filter((entry) => entry.draft.name.trim() !== "" || entry.draft.taskPrefix.trim() !== "")
      .map((entry) => ({
        name: entry.draft.name.trim(),
        taskPrefix: entry.draft.taskPrefix.trim().toUpperCase(),
        description: entry.draft.description,
        guidelines: entry.draft.guidelines,
        isLeader: entry.draft.isLeader,
      }));
    create.mutate({ name, description, workspacePath, roles });
  };

  const nameError = submitted && name.trim() === "" ? "请填写团队名称" : undefined;
  const exampleList: RoleTemplate[] = templates.data ?? [];

  return (
    <div className="tc-page">
      <PageHeader
        title="创建团队"
        description="给一组岗位起个名字，指定一个本地文件夹作为共同工作空间。"
        actions={
          <Button loading={create.isPending} onClick={submit}>
            创建团队
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
                label="团队名称"
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
            </div>
          </Card>

          <Card tone="info" title="本地工作空间" className="tc-stack-card">
            <div className="tc-stack">
              <p className="tc-note">任意本地文件夹；若是 Git 仓库会显示分支与提交。</p>
              <p>
                <span className="nowrap">路径：</span>
                {workspacePath.trim() === "" ? "尚未选择工作空间" : workspacePath}
              </p>
              <p>{workspace === null ? "尚未检测" : workspaceLine(workspace)}</p>
            </div>
          </Card>

          <div className="tc-stack">
            <InputField
              label="手动输入工作空间路径"
              value={workspacePath}
              placeholder="/Users/you/projects/app"
              hint="填写后点「检测」按该路径重新检测。"
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
            2. 建立岗位
          </h2>

          <div className="tc-actions">
            <Button
              variant="neutral"
              onClick={() => {
                addDraft();
              }}
            >
              + 添加岗位
            </Button>
            <Button
              variant="subtle"
              onClick={() => {
                setShowExamples((previous) => !previous);
              }}
            >
              从示例添加
            </Button>
          </div>

          {showExamples ? (
            <div className="tc-examples">
              {templates.isPending ? <p className="tc-note">正在加载示例…</p> : null}
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
              {!templates.isPending && !templates.isError && exampleList.length === 0 ? (
                <p className="tc-note">没有可用的示例岗位，直接用「+ 添加岗位」自定义即可。</p>
              ) : null}
              {exampleList.length > 0 ? (
                <ul className="tc-example-list">
                  {exampleList.map((template) => (
                    <li key={template.code} className="tc-example">
                      <div className="tc-example__info">
                        <span className="tc-example__name">{template.name}</span>
                        <span className="tc-note">{template.description}</span>
                      </div>
                      <Button
                        variant="neutral"
                        onClick={() => {
                          addDraft(draftFromTemplate(template));
                        }}
                      >
                        添加
                      </Button>
                    </li>
                  ))}
                </ul>
              ) : null}
            </div>
          ) : null}

          {entries.length === 0 && !showExamples ? (
            <EmptyState
              bare
              title="还没有岗位"
              description="岗位完全由你定义；也可以先创建团队，稍后在「岗位与绑定」中添加。"
            />
          ) : null}

          {entries.map((entry, index) => {
            const draft = entry.draft;
            return (
              <Card
                key={entry.key}
                className="tc-stack-card tc-role"
                title={`岗位 ${String(index + 1)}`}
              >
                <div className="tc-stack tc-role__fields">
                  <InputField
                    label="岗位名称"
                    required
                    value={draft.name}
                    error={roleNameError(draft.name, submitted)}
                    placeholder="例如：资料整理"
                    onChange={(event) => {
                      patchDraft(entry.key, { name: event.currentTarget.value });
                    }}
                    onBlur={() => {
                      suggestPrefix(entry.key, draft.name, (resolvePrefix) => {
                        // 函数式 setState：按返回那一刻的行前缀判定，不用失焦时的快照。
                        setEntries((previous) =>
                          previous.map((item) =>
                            item.key === entry.key
                              ? {
                                  ...item,
                                  draft: {
                                    ...item.draft,
                                    taskPrefix: resolvePrefix(item.draft.taskPrefix),
                                  },
                                }
                              : item,
                          ),
                        );
                      });
                    }}
                  />
                  <InputField
                    label="任务编号前缀"
                    required
                    hint={TASK_PREFIX_HINT}
                    value={draft.taskPrefix}
                    onChange={(event) => {
                      patchDraft(entry.key, { taskPrefix: event.currentTarget.value });
                    }}
                  />
                  <TextareaField
                    label="职责"
                    value={draft.description}
                    placeholder="这个岗位负责什么"
                    onChange={(event) => {
                      patchDraft(entry.key, { description: event.currentTarget.value });
                    }}
                  />
                  <TextareaField
                    label="工作约定"
                    value={draft.guidelines}
                    placeholder="协作时希望遵守的约定"
                    onChange={(event) => {
                      patchDraft(entry.key, { guidelines: event.currentTarget.value });
                    }}
                  />
                  <label className="tc-role__leader">
                    <input
                      className="tc-role__checkbox"
                      type="checkbox"
                      checked={draft.isLeader}
                      onChange={(event) => {
                        patchDraft(entry.key, { isLeader: event.currentTarget.checked });
                      }}
                    />
                    <span>设为 Leader</span>
                  </label>
                  <div className="tc-actions">
                    <Button
                      variant="subtle"
                      onClick={() => {
                        removeDraft(entry.key);
                      }}
                    >
                      删除岗位
                    </Button>
                  </div>
                </div>
              </Card>
            );
          })}

          <p className="tc-note">
            可以不建任何岗位直接创建团队，也可以稍后在「岗位与绑定」中添加。
          </p>
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
