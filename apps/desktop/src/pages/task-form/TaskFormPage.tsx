import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent } from "react";
import { useNavigate, useParams } from "react-router";

import type { FileRef } from "../../bindings/FileRef.js";
import type { HandshakeStatus } from "../../bindings/HandshakeStatus.js";
import type { RoleView } from "../../bindings/RoleView.js";
import type { TaskStatus } from "../../bindings/TaskStatus.js";
import type { TaskSummary } from "../../bindings/TaskSummary.js";
import {
  checkTaskDependencies,
  createTask,
  getTask,
  getTaskBoard,
  listRoles,
  listTaskStatuses,
  searchWorkspaceFiles,
  updateTask,
} from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  Card,
  EmptyState,
  ErrorBanner,
  InputField,
  PageHeader,
  Panel,
  SelectField,
  Spinner,
  Tag,
  TextareaField,
} from "../../components/index.js";
import type { TagTone } from "../../components/index.js";
import { TaskDependencyChain } from "../../components/TaskDependencyChain.js";
import { isRunActive } from "../task-detail/run/useRunEvents.js";
import "./task-form.css";

/** 与后端 update_task 在有活动执行时的拒绝原因一致。 */
const EDIT_BLOCKED_TEXT = "任务正在执行中，结束或取消执行后才能编辑";

/** 创建任务允许的初始状态（其余状态由任务详情页的流转操作产生）。 */
const INITIAL_STATUSES: readonly TaskStatus[] = ["todo", "ready"];

/** 工作区文件搜索的 query key。queryKeys.ts 里没有这一项，且本任务不允许改共享文件，故在页面内定义；形状与其他 key 一致（[资源名, ...参数]）。 */
function fileSearchKey(teamId: string, query: string) {
  return ["teams", teamId, "files", query] as const;
}

function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebounced(value);
    }, delayMs);
    return () => {
      clearTimeout(timer);
    };
  }, [value, delayMs]);
  return debounced;
}

function statusTone(status: TaskStatus): TagTone {
  switch (status) {
    case "done":
      return "success";
    case "blocked":
      return "danger";
    case "doing":
    case "review":
      return "warning";
    case "ready":
      return "info";
    default:
      return "neutral";
  }
}

function healthLabel(status: HandshakeStatus): string {
  if (status === "connected") {
    return "已连接";
  }
  if (status === "needs_auth") {
    return "需要登录";
  }
  if (status === "error") {
    return "异常";
  }
  return "未检查";
}

function harnessKindLabel(kind: string): string {
  if (kind === "claude_code") {
    return "Claude Code";
  }
  if (kind === "codex") {
    return "Codex";
  }
  if (kind === "minimax_code") {
    return "MiniMax Code";
  }
  if (kind === "gemini_cli") {
    return "Gemini CLI";
  }
  if (kind === "codebuddy") {
    return "CodeBuddy（腾讯）";
  }
  if (kind === "qoder_cli") {
    return "Qoder CLI（阿里）";
  }
  if (kind === "grok_build") {
    return "Grok Build（xAI）";
  }
  if (kind === "qwen_code") {
    return "Qwen Code（阿里）";
  }
  if (kind === "kimi_cli") {
    return "Kimi CLI（月之暗面）";
  }
  if (kind === "opencode") {
    return "OpenCode";
  }
  if (kind === "custom") {
    return "自定义 ACP Agent";
  }
  return "Mock";
}

/** 把 `path#锚点` 拆成 FileRef；锚点为空时写 null。 */
function parseFileRef(raw: string): FileRef | null {
  const trimmed = raw.trim();
  if (trimmed === "") {
    return null;
  }
  const hash = trimmed.indexOf("#");
  const path = hash === -1 ? trimmed : trimmed.slice(0, hash);
  const anchor = hash === -1 ? "" : trimmed.slice(hash + 1);
  if (path === "") {
    return null;
  }
  return { path, anchor: anchor === "" ? null : anchor };
}

function fileRefText(ref: FileRef): string {
  return ref.anchor === null || ref.anchor === undefined ? ref.path : `${ref.path}#${ref.anchor}`;
}

export function TaskFormPage() {
  const params = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const teamId = params.teamId ?? "";
  const taskId = params.taskId;
  const isEdit = taskId !== undefined;
  const ready = teamId !== "";

  const [title, setTitle] = useState("");
  const [ownerRoleId, setOwnerRoleId] = useState("");
  const [description, setDescription] = useState("");
  const [fileRefs, setFileRefs] = useState<FileRef[]>([]);
  const [fileInput, setFileInput] = useState("");
  const [dependsOn, setDependsOn] = useState<string[]>([]);
  const [initialStatus, setInitialStatus] = useState<TaskStatus>("todo");
  const [submitted, setSubmitted] = useState(false);

  const rolesQuery = useQuery({
    queryKey: queryKeys.roles(teamId),
    queryFn: () => listRoles(teamId),
    enabled: ready,
  });

  const statusesQuery = useQuery({
    queryKey: queryKeys.taskStatuses,
    queryFn: listTaskStatuses,
    enabled: ready,
  });

  const boardQueryInput = { teamId };
  const boardQuery = useQuery({
    queryKey: queryKeys.taskBoard(teamId, boardQueryInput),
    queryFn: () => getTaskBoard(boardQueryInput),
    enabled: ready,
  });

  const detailQuery = useQuery({
    queryKey: queryKeys.task(teamId, taskId ?? ""),
    queryFn: () => getTask(taskId ?? ""),
    enabled: ready && isEdit,
  });

  const debouncedFileInput = useDebouncedValue(fileInput.trim(), 200);
  const fileSearchQuery = useQuery({
    queryKey: fileSearchKey(teamId, debouncedFileInput),
    queryFn: () => searchWorkspaceFiles({ teamId, query: debouncedFileInput }),
    enabled: ready && debouncedFileInput !== "",
    staleTime: 30_000,
  });

  // 预填只在对应任务的数据首次到达时执行一次，之后的重新拉取不覆盖用户正在编辑的内容。
  const prefilledFor = useRef<string | null>(null);
  const detail = detailQuery.data;
  const dependencySelection = useMemo(() => [...dependsOn].sort(), [dependsOn]);
  const debouncedDependencies = useDebouncedValue(dependencySelection, 200);
  const dependenciesDebouncing =
    dependencySelection.length !== debouncedDependencies.length ||
    dependencySelection.some((id, index) => id !== debouncedDependencies[index]);
  const dependencyCheck = useQuery({
    queryKey: ["task-dependency-check", teamId, taskId ?? null, debouncedDependencies],
    queryFn: () =>
      checkTaskDependencies({
        teamId,
        taskId: taskId ?? null,
        dependsOn: debouncedDependencies,
      }),
    enabled: ready && (!isEdit || detail !== undefined),
    retry: false,
    staleTime: 0,
  });
  const cycle = dependenciesDebouncing ? [] : (dependencyCheck.data?.cycle ?? []);
  const dependenciesValid =
    !dependenciesDebouncing &&
    dependencyCheck.isSuccess &&
    !dependencyCheck.isFetching &&
    cycle.length === 0;
  useEffect(() => {
    if (detail === undefined || prefilledFor.current === detail.task.id) {
      return;
    }
    prefilledFor.current = detail.task.id;
    setTitle(detail.task.title);
    setOwnerRoleId(detail.task.ownerRoleId);
    setDescription(detail.task.description);
    setDependsOn([...detail.task.dependsOn]);
    setFileRefs(detail.fileRefs.map((ref) => ({ path: ref.path, anchor: ref.anchor })));
  }, [detail]);

  interface TaskFormPayload {
    title: string;
    description: string;
    ownerRoleId: string;
    fileRefs: FileRef[];
    dependsOn: string[];
    status: TaskStatus;
  }

  const saveMutation = useMutation({
    mutationFn: (payload: TaskFormPayload) => {
      if (isEdit) {
        return updateTask({
          taskId,
          title: payload.title,
          description: payload.description,
          ownerRoleId: payload.ownerRoleId,
          fileRefs: payload.fileRefs,
          dependsOn: payload.dependsOn,
        });
      }
      return createTask({
        teamId,
        title: payload.title,
        description: payload.description,
        ownerRoleId: payload.ownerRoleId,
        status: payload.status,
        fileRefs: payload.fileRefs,
        dependsOn: payload.dependsOn,
      });
    },
    onSuccess: (saved) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.tasks(teamId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.roles(teamId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.teamOverview(teamId) });
      void navigate(`/teams/${teamId}/tasks/${saved.task.id}`, { replace: true });
    },
  });

  if (!ready) {
    return (
      <>
        <PageHeader title={isEdit ? "编辑任务" : "创建任务"} />
        <EmptyState title="缺少团队" description="请从左侧选择一个团队后再创建或编辑任务。" />
      </>
    );
  }

  const roles = rolesQuery.data ?? [];
  const selectedRole: RoleView | undefined = roles.find((role) => role.role.id === ownerRoleId);
  const selectedHarness = selectedRole?.harness ?? null;

  const statusLabels = new Map<TaskStatus, string>(
    (statusesQuery.data ?? []).map((info) => [info.status, info.label]),
  );
  const labelOf = (status: TaskStatus): string => statusLabels.get(status) ?? status;

  const boardColumns = boardQuery.data?.columns ?? [];
  const boardLabel = new Map<TaskStatus, string>(
    boardColumns.map((column) => [column.status, column.label]),
  );
  const dependencyOptions: TaskSummary[] = boardColumns.flatMap((column) => column.tasks);
  // 编辑时不能把自己列为依赖（否则必然成环）；这里只是不给选项，规则仍由后端判定。
  const dependencyChoices = dependencyOptions.filter((task) => task.id !== taskId);
  const dependencyById = new Map(dependencyOptions.map((task) => [task.id, task]));

  const addedPaths = new Set(fileRefs.map((ref) => ref.path));
  const suggestions = (fileSearchQuery.data ?? []).filter((path) => !addedPaths.has(path));
  const searchReady = debouncedFileInput !== "";
  const searchPending = searchReady && fileSearchQuery.isPending;

  const initialStatuses = INITIAL_STATUSES.map((status) => ({
    status,
    label: labelOf(status),
  }));

  const addFileRef = (raw: string) => {
    const ref = parseFileRef(raw);
    if (ref === null) {
      return;
    }
    setFileRefs((previous) =>
      previous.some((item) => fileRefText(item) === fileRefText(ref))
        ? previous
        : [...previous, ref],
    );
    setFileInput("");
  };

  const removeFileRef = (ref: FileRef) => {
    setFileRefs((previous) => previous.filter((item) => fileRefText(item) !== fileRefText(ref)));
  };

  const toggleDependency = (id: string, checked: boolean) => {
    setDependsOn((previous) =>
      checked ? [...previous, id] : previous.filter((item) => item !== id),
    );
  };

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!dependenciesValid) {
      return;
    }
    setSubmitted(true);
    if (title.trim() === "") {
      // 标题为空时前端直接拦下，不发请求；错误走 InputField 的 error 通道。
      return;
    }
    saveMutation.mutate({
      title,
      description,
      ownerRoleId,
      fileRefs,
      dependsOn,
      status: initialStatus,
    });
  };

  const cancel = () => {
    void navigate(`/teams/${teamId}/tasks`);
  };

  const busy = rolesQuery.isPending || (isEdit && detailQuery.isPending);
  const canSubmit = !busy && !saveMutation.isPending && dependenciesValid;
  const detailBlocked = isEdit && detailQuery.isError;
  // 有活动执行时后端会拒绝编辑；这里提前提示并禁用保存，文案与后端一致。
  const editBlocked = isEdit && isRunActive(detail?.latestRun ?? null);

  return (
    <>
      <PageHeader
        title={isEdit ? "编辑任务" : "创建任务"}
        description={
          isEdit
            ? "修改任务面板保存的结构化协作信息；任务状态在任务详情页变更。"
            : "任务面板保存结构化协作信息；需求正文放在工作空间的文件里。"
        }
      />

      <form className="task-form" onSubmit={submit} noValidate>
        {editBlocked ? (
          <div className="ui-error-banner" role="alert">
            <span>{EDIT_BLOCKED_TEXT}</span>
          </div>
        ) : null}
        {saveMutation.isError ? <ErrorBanner error={saveMutation.error} /> : null}

        {isEdit && detailQuery.isPending ? (
          <div className="task-form__loading">
            <Spinner label="正在加载任务…" />
          </div>
        ) : null}

        {isEdit && detailQuery.isError ? <ErrorBanner error={detailQuery.error} /> : null}

        <Panel className="task-form__block">
          <InputField
            label="任务名称"
            required
            value={title}
            error={submitted && title.trim() === "" ? "请填写任务名称" : undefined}
            onChange={(event) => {
              setTitle(event.target.value);
            }}
            placeholder="例如：实现注册界面"
            hint="一句话说明任务目标，会显示在看板卡片和任务详情页。"
          />
        </Panel>

        <Panel className="task-form__block">
          {rolesQuery.isPending ? (
            <div className="task-form__loading">
              <Spinner label="正在加载岗位…" />
            </div>
          ) : null}

          {rolesQuery.isError ? (
            <ErrorBanner
              error={rolesQuery.error}
              action={
                <Button variant="neutral" onClick={() => void rolesQuery.refetch()}>
                  重试
                </Button>
              }
            />
          ) : null}

          {rolesQuery.isSuccess && roles.length === 0 ? (
            <EmptyState
              title="还没有可用岗位"
              description="任务必须指定负责岗位，请先在「岗位与绑定」里添加岗位。"
              action={
                <Button
                  variant="neutral"
                  onClick={() => {
                    void navigate(`/teams/${teamId}/roles`);
                  }}
                >
                  去添加岗位
                </Button>
              }
            />
          ) : null}

          {rolesQuery.isSuccess && roles.length > 0 ? (
            <SelectField
              label="负责人岗位"
              required
              value={ownerRoleId}
              onChange={(event) => {
                setOwnerRoleId(event.target.value);
              }}
            >
              <option value="">请选择负责岗位</option>
              {roles.map((roleView) => (
                <option key={roleView.role.id} value={roleView.role.id}>
                  {roleView.role.name}
                </option>
              ))}
            </SelectField>
          ) : null}

          {selectedRole !== undefined ? (
            <p className="task-form__binding">
              岗位标识：{selectedRole.role.code} ·{" "}
              {selectedHarness === null ? (
                <>未绑定 Harness · 暂不派工</>
              ) : (
                <>
                  当前 Binding：{selectedHarness.name}（{harnessKindLabel(selectedHarness.preset)} ·{" "}
                  {healthLabel(selectedHarness.handshake.status)}）
                </>
              )}
            </p>
          ) : null}

          {selectedHarness !== null ? (
            <p className="task-form__note">
              预计由当前绑定 {selectedHarness.name} 领取。
              岗位未绑定或并发已满时，任务会排队，不会丢失。
            </p>
          ) : null}
        </Panel>

        <Panel className="task-form__block">
          <div className="task-form__files">
            <label className="task-form__label" htmlFor="task-file-search">
              关联文件（可选）
            </label>

            <div className="task-form__file-controls">
              <input
                id="task-file-search"
                className="ui-field__control"
                value={fileInput}
                placeholder="搜索工作空间内的文件，或直接输入 路径#锚点"
                onChange={(event) => {
                  setFileInput(event.target.value);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    addFileRef(fileInput);
                  }
                }}
              />
              <Button
                variant="neutral"
                disabled={fileInput.trim() === ""}
                onClick={() => {
                  addFileRef(fileInput);
                }}
              >
                添加
              </Button>
            </div>

            <p className="ui-field__hint">
              输入时实时搜索工作空间内的文件；用 # 追加锚点可直达具体章节，引用文件需包含验收标准。
            </p>

            {searchPending ? (
              <div className="task-form__loading">
                <Spinner label="正在搜索文件…" />
              </div>
            ) : null}

            {searchReady && fileSearchQuery.isError ? (
              <ErrorBanner
                error={fileSearchQuery.error}
                action={
                  <Button variant="neutral" onClick={() => void fileSearchQuery.refetch()}>
                    重试
                  </Button>
                }
              />
            ) : null}

            {searchReady && fileSearchQuery.isSuccess && suggestions.length === 0 ? (
              <p className="ui-field__hint">没有匹配的文件；可以直接输入工作空间内的相对路径。</p>
            ) : null}

            {suggestions.length > 0 ? (
              <ul className="task-form__suggestions">
                {suggestions.map((path) => (
                  <li key={path}>
                    <button
                      type="button"
                      className="task-form__suggestion"
                      onClick={() => {
                        addFileRef(path);
                      }}
                    >
                      {path}
                    </button>
                  </li>
                ))}
              </ul>
            ) : null}

            {fileRefs.length === 0 ? (
              <p className="ui-field__hint">尚未添加引用。</p>
            ) : (
              <ul className="task-form__chips">
                {fileRefs.map((ref) => (
                  <li key={fileRefText(ref)} className="task-form__chip">
                    <span className="task-form__chip-text">{fileRefText(ref)}</span>
                    <button
                      type="button"
                      className="task-form__chip-remove"
                      aria-label={`移除引用 ${fileRefText(ref)}`}
                      onClick={() => {
                        removeFileRef(ref);
                      }}
                    >
                      移除
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </Panel>

        <Panel className="task-form__block">
          <TextareaField
            label="说明"
            rows={6}
            value={description}
            onChange={(event) => {
              setDescription(event.target.value);
            }}
            placeholder="填写协作要点、验收标准或需要交接的上下文。"
            hint="详细需求保留在文件中，这里只登记协作信息。"
          />
        </Panel>

        <Card title="依赖与初始状态" tone="info" className="task-form__info">
          <p className="task-form__summary">
            依赖：
            {dependsOn.length === 0
              ? "无"
              : dependsOn
                  .map((id) => {
                    const task = dependencyById.get(id);
                    const status = task === undefined ? "" : (boardLabel.get(task.status) ?? "");
                    return task === undefined ? id : `${task.key}（${status}）`;
                  })
                  .join("、")}
          </p>
          <p className="task-form__summary">
            初始：
            {isEdit
              ? `${labelOf(detail?.task.status ?? "todo")} · 状态变更在任务详情页进行`
              : `${labelOf(initialStatus)} · ${
                  initialStatus === "ready" ? "可由负责人领取" : "暂不可领取"
                }`}
          </p>

          <div className="task-form__dependencies">
            {boardQuery.isPending ? (
              <div className="task-form__loading">
                <Spinner label="正在加载任务…" />
              </div>
            ) : null}

            {boardQuery.isError ? (
              <ErrorBanner
                error={boardQuery.error}
                action={
                  <Button variant="neutral" onClick={() => void boardQuery.refetch()}>
                    重试
                  </Button>
                }
              />
            ) : null}

            {boardQuery.isSuccess ? (
              <fieldset
                className="task-form__fieldset"
                aria-invalid={cycle.length > 0}
                aria-describedby={cycle.length > 0 ? "dependency-cycle-error" : undefined}
              >
                <legend className="task-form__label">依赖</legend>
                {dependencyOptions.length === 0 ? (
                  <EmptyState
                    title="本团队还没有任务"
                    description="先创建前置任务，再回来把它设为依赖。"
                  />
                ) : dependencyChoices.length === 0 ? (
                  <p className="ui-field__hint">没有其他可添加的任务。</p>
                ) : (
                  <ul className="task-form__options">
                    {dependencyChoices.map((task) => {
                      const alreadyOn = dependsOn.includes(task.id);
                      return (
                        <li key={task.id}>
                          <label
                            className={
                              alreadyOn
                                ? "task-form__option task-form__option--on"
                                : "task-form__option"
                            }
                          >
                            <input
                              type="checkbox"
                              checked={alreadyOn}
                              disabled={alreadyOn}
                              onChange={(event) => {
                                toggleDependency(task.id, event.target.checked);
                              }}
                            />
                            <span className="task-form__option-key">{task.key}</span>
                            <span className="task-form__option-title">{task.title}</span>
                            <Tag tone={statusTone(task.status)}>
                              {boardLabel.get(task.status) ?? labelOf(task.status)}
                            </Tag>
                          </label>
                        </li>
                      );
                    })}
                  </ul>
                )}
              </fieldset>
            ) : null}

            {dependsOn.length > 0 ? (
              <ul className="task-form__chips">
                {dependsOn.map((id) => {
                  const task = dependencyById.get(id);
                  const label = task === undefined ? id : `${task.key} · ${task.title}`;
                  return (
                    <li key={id} className="task-form__chip">
                      <span className="task-form__chip-text">{label}</span>
                      {task === undefined ? null : (
                        <Tag tone={cycle.length > 0 ? "danger" : statusTone(task.status)}>
                          {boardLabel.get(task.status) ?? labelOf(task.status)}
                        </Tag>
                      )}
                      <button
                        type="button"
                        className="task-form__chip-remove"
                        aria-label={`移除依赖 ${label}`}
                        onClick={() => {
                          toggleDependency(id, false);
                        }}
                      >
                        移除
                      </button>
                    </li>
                  );
                })}
              </ul>
            ) : null}
            {dependenciesDebouncing || dependencyCheck.isFetching ? (
              <p role="status">正在检查依赖…</p>
            ) : null}
            {!dependenciesDebouncing && dependencyCheck.isError ? (
              <ErrorBanner
                error={dependencyCheck.error}
                action={
                  <Button variant="neutral" onClick={() => void dependencyCheck.refetch()}>
                    重试依赖检查
                  </Button>
                }
              />
            ) : cycle.length > 0 ? (
              <div className="dependency-cycle" id="dependency-cycle-error" role="alert">
                <strong>不能保存：依赖形成了循环</strong>
                <span>依赖形成环路：{cycle.join(" → ")}</span>
                <p>请移除环路中的一条依赖。保存时后端仍会重新校验，拒绝环路并保留原有依赖。</p>
              </div>
            ) : dependsOn.length > 0 || (detail?.dependents.length ?? 0) > 0 ? (
              <TaskDependencyChain
                preview
                teamId={teamId}
                current={{
                  id: taskId ?? "",
                  key: detail?.task.key ?? "新任务",
                  title,
                  status: detail?.task.status ?? initialStatus,
                }}
                dependencies={dependsOn.flatMap((id) => {
                  const task = dependencyById.get(id);
                  return task === undefined ? [] : [task];
                })}
                dependents={detail?.dependents ?? []}
                statusLabel={(status) => boardLabel.get(status) ?? labelOf(status)}
              />
            ) : null}
            {dependsOn.length > 0 && dependenciesValid ? (
              <Card title="自动就绪" tone="info">
                <p>{dependencyCheck.data.readinessNotice}</p>
              </Card>
            ) : null}
          </div>

          {!isEdit ? (
            <fieldset className="task-form__fieldset">
              <legend className="task-form__label">初始状态</legend>
              <div className="task-form__statuses">
                {initialStatuses.map((item) => (
                  <label key={item.status} className="task-form__option">
                    <input
                      type="radio"
                      name="task-initial-status"
                      checked={initialStatus === item.status}
                      onChange={() => {
                        setInitialStatus(item.status);
                      }}
                    />
                    <span>{item.label}</span>
                  </label>
                ))}
              </div>
              <p className="ui-field__hint">
                只有 TODO 和 READY 可以作为初始状态；其他状态在任务详情页流转。
              </p>
            </fieldset>
          ) : null}
        </Card>

        <p className="task-form__note task-form__note--rules">
          READY 必须具备负责人与已满足的依赖；填写的引用文件必须存在。循环依赖会阻止保存。
        </p>

        <div className="task-form__actions">
          <Button
            type="submit"
            loading={saveMutation.isPending}
            disabled={!canSubmit || detailBlocked || editBlocked}
          >
            {isEdit ? "保存" : "创建并分配"}
          </Button>
          <Button variant="neutral" disabled={saveMutation.isPending} onClick={cancel}>
            取消
          </Button>
        </div>
      </form>
    </>
  );
}
