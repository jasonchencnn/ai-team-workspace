import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate, useParams } from "react-router";

import {
  getRoleWorkbench,
  listHarnesses,
  listTaskStatuses,
  openRolePath,
} from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import type { HarnessInstance } from "../../bindings/HarnessInstance.js";
import type { RoleFileEntry } from "../../bindings/RoleFileEntry.js";
import type { Run } from "../../bindings/Run.js";
import type { TaskStatus } from "../../bindings/TaskStatus.js";
import type { TaskSummary } from "../../bindings/TaskSummary.js";
import {
  Button,
  Card,
  EmptyState,
  ErrorBanner,
  PageHeader,
  Panel,
  Spinner,
  Tag,
} from "../../components/index.js";
import {
  agentVersion,
  formatRelative,
  handshakeDetailTone,
  healthLabel,
  healthTone,
  isTestHarness,
  presetTagLabel,
} from "../harness-detail/harnessFormat.js";
import { PendingBindingCard } from "../roles/PendingBindingCard.js";
import { PendingMessagesPanel } from "../roles/PendingMessagesPanel.js";
import { RebindHarnessDialog } from "../roles/RebindHarnessDialog.js";
import { splitJournalLine, useRevealRoleFolder } from "../roles/roleFolder.js";
import { roleBindingPath } from "../roles/rolesNav.js";
import {
  REBIND_POLL_INTERVAL_MS,
  useRoleActiveRuns,
  useRebindRefresh,
} from "../roles/useRebindData.js";
import { useRoleMutations } from "../roles/useRoleMutations.js";
import { runDisplayStatus } from "../task-detail/run/runUi.js";
import { statusTone } from "../tasks/taskUi.js";
import { formatLocalDateTime } from "../team-home/formatLocalDateTime.js";
import "./role-workbench.css";

/** 工作日志文件名：Teamspace 自动维护，按钮直接在 Finder 中定位它。 */
const JOURNAL_FILE = "工作日志.md";

/**
 * 岗位工作台的查询 key 挂在岗位 key 之下，因此岗位编辑、绑定、解绑成功后
 * `useRoleMutations` 失效岗位 key 时会一并刷新这里，无需页面各自补一次刷新。
 */
function workbenchKey(teamId: string, roleId: string) {
  return [...queryKeys.roles(teamId), "workbench", roleId] as const;
}

export function RoleWorkbenchPage() {
  const { teamId, roleId } = useParams();
  const navigate = useNavigate();

  if (teamId === undefined || roleId === undefined) {
    return <EmptyState title="未选择岗位" description="请先在团队空间选择一个岗位。" />;
  }

  return (
    <WorkbenchView
      key={roleId}
      teamId={teamId}
      roleId={roleId}
      onBack={() => {
        void navigate(`/teams/${teamId}`);
      }}
    />
  );
}

interface WorkbenchViewProps {
  teamId: string;
  roleId: string;
  onBack: () => void;
}

function WorkbenchView({ teamId, roleId, onBack }: WorkbenchViewProps) {
  const navigate = useNavigate();
  const [rebindOpen, setRebindOpen] = useState(false);

  const workbench = useQuery({
    queryKey: workbenchKey(teamId, roleId),
    queryFn: () => getRoleWorkbench(roleId),
    // 有待生效换绑时轮询：当前执行结束就自动生效，工作台要跟上。
    refetchInterval: (query) =>
      query.state.data?.view.pendingBinding !== null &&
      query.state.data?.view.pendingBinding !== undefined
        ? REBIND_POLL_INTERVAL_MS
        : false,
  });

  // 换绑流程要选目标实例：同团队的实例列表与岗位页共用同一份缓存。
  const harnessesQuery = useQuery({
    queryKey: queryKeys.harnesses(teamId),
    queryFn: () => listHarnesses(teamId),
  });
  const activeRunsQuery = useRoleActiveRuns(teamId, roleId);
  const { rebindRole, cancelPendingRebind } = useRoleMutations(teamId);

  // 换绑生效 / 撤销时刷新工作台（岗位视图挂在岗位 key 之下）。
  useRebindRefresh(teamId);

  // 任务状态显示名一律来自后端。
  const statuses = useQuery({ queryKey: queryKeys.taskStatuses, queryFn: listTaskStatuses });
  const statusLabel = new Map((statuses.data ?? []).map((item) => [item.status, item.label]));

  const reveal = useRevealRoleFolder();
  const openPath = useMutation({
    mutationFn: (relativePath: string) => openRolePath(roleId, relativePath),
  });

  if (workbench.isPending) {
    return <Spinner label="正在加载岗位工作台" />;
  }

  if (workbench.isError) {
    return (
      <ErrorBanner
        error={workbench.error}
        action={
          <Button
            variant="neutral"
            onClick={() => {
              void workbench.refetch();
            }}
          >
            重试
          </Button>
        }
      />
    );
  }

  const data = workbench.data;
  const role = data.view.role;
  const folder = data.folder;
  // 岗位列表带回的同步失败（view.folderProblem）与本次查询读到的失败（folder.problem）
  // 是两个来源，都可能是中文说明，去重后一起显示。
  const problems = [...new Set([data.view.folderProblem, folder.problem])].filter(
    (item): item is string => item !== null,
  );
  const folderLabel = folder.relativePath ?? data.view.folder;

  const goRoles = () => {
    void navigate(roleBindingPath(teamId, roleId));
  };

  return (
    <div className="rw-page">
      <PageHeader
        title={role.name}
        actions={
          <>
            <Button variant="neutral" onClick={onBack}>
              返回团队空间
            </Button>
            <Button
              loading={reveal.isPending && reveal.variables === roleId}
              onClick={() => {
                reveal.mutate(roleId);
              }}
            >
              在 Finder 中打开岗位文件夹
            </Button>
          </>
        }
      />

      <p className="rw-meta">
        <span className="rw-meta__item">
          <span className="rw-meta__label">任务前缀</span>
          <span className="rw-meta__prefix">{role.taskPrefix}</span>
        </span>
        <span className="rw-meta__item">{role.isLeader ? "Leader 岗位" : "普通岗位"}</span>
        <span className="rw-meta__item rw-meta__code" title="岗位标识，创建后不可修改">
          {role.code}
        </span>
      </p>

      {reveal.error === null ? null : <ErrorBanner error={reveal.error} />}

      <div className="rw-columns">
        <div className="rw-column">
          <RecentRuns
            teamId={teamId}
            runs={data.recentRuns}
            onOpen={(taskId) => {
              void navigate(`/teams/${teamId}/tasks/${taskId}`);
            }}
          />

          <RecentTasks
            teamId={teamId}
            tasks={data.recentTasks}
            taskCount={data.taskCount}
            statusLabel={statusLabel}
          />

          <Panel className="rw-panel">
            <div className="rw-panel__head">
              <h2 className="rw-panel__title">工作日志</h2>
              <Button
                variant="neutral"
                loading={openPath.isPending && openPath.variables === JOURNAL_FILE}
                onClick={() => {
                  openPath.mutate(JOURNAL_FILE);
                }}
              >
                {`打开${JOURNAL_FILE}`}
              </Button>
            </div>

            {folder.journal.length === 0 ? (
              <EmptyState
                bare
                title="暂无记录"
                description="岗位建立、派工和状态变化会自动记入这里。"
              />
            ) : (
              <ol className="rw-journal">
                {folder.journal.map((line, index) => {
                  const entry = splitJournalLine(line);
                  return (
                    <li key={`${String(index)}-${entry.time}`} className="rw-journal-item">
                      <span className="rw-journal-dot" aria-hidden="true" />
                      {entry.time === "" ? null : (
                        <span className="rw-journal-time">{entry.time}</span>
                      )}
                      <span className="rw-journal-text">{entry.text}</span>
                    </li>
                  );
                })}
              </ol>
            )}

            {openPath.error === null || openPath.variables !== JOURNAL_FILE ? null : (
              <ErrorBanner error={openPath.error} />
            )}
          </Panel>
        </div>

        <div className="rw-column">
          <Panel className="rw-panel">
            <h2 className="rw-panel__title">当前绑定</h2>
            {data.view.harness === null ? (
              <>
                <p className="rw-binding__unbound">
                  <Tag tone="warning">未绑定</Tag>
                  <span>暂不派工</span>
                </p>
                <p className="rw-note">
                  该岗位没有绑定 Harness，新任务不会派给它，已有记录仍可查看。
                </p>
                <div className="rw-actions">
                  <Button onClick={goRoles}>绑定 Harness</Button>
                </div>
              </>
            ) : (
              <BindingBody
                harness={data.view.harness}
                onChange={goRoles}
                onRebind={() => {
                  rebindRole.reset();
                  setRebindOpen(true);
                }}
              />
            )}
          </Panel>

          <PendingBindingCard
            view={data.view}
            isCancelling={cancelPendingRebind.isPending}
            error={cancelPendingRebind.error}
            onCancel={() => {
              cancelPendingRebind.mutate(roleId);
            }}
          />

          <PendingMessagesPanel
            teamId={teamId}
            roleId={roleId}
            enabled={data.view.pendingBinding !== null}
          />

          {problems.length === 0 ? null : (
            <Card tone="warning" title="岗位文件夹无法写入">
              {problems.map((problem) => (
                <p key={problem} className="rw-note">
                  {problem}
                </p>
              ))}
              <p className="rw-note">工作日志暂存在应用内，恢复后自动补写；岗位仍可正常派工。</p>
              <div className="rw-actions">
                <Button
                  variant="neutral"
                  onClick={() => {
                    void workbench.refetch();
                  }}
                >
                  重新检查
                </Button>
                <Button
                  variant="neutral"
                  loading={reveal.isPending && reveal.variables === roleId}
                  onClick={() => {
                    reveal.mutate(roleId);
                  }}
                >
                  在 Finder 中打开
                </Button>
              </div>
            </Card>
          )}

          <Panel className="rw-panel">
            <h2 className="rw-panel__title">岗位文件夹</h2>
            <p className="rw-path" title={folder.path ?? folderLabel ?? undefined}>
              {folderLabel ?? "尚未建立"}
            </p>
            {role.description.trim() === "" ? null : (
              <>
                <h3 className="rw-panel__subtitle">职责</h3>
                <p className="rw-note">{role.description}</p>
              </>
            )}
            {role.guidelines.trim() === "" ? null : (
              <>
                <h3 className="rw-panel__subtitle">工作约定</h3>
                <p className="rw-note">{role.guidelines}</p>
              </>
            )}
          </Panel>

          <Panel className="rw-panel">
            <h2 className="rw-panel__title">最近文件</h2>
            {data.recentFiles.length === 0 ? (
              <EmptyState
                bare
                title="暂无过程文件"
                description="岗位执行任务产出的文件保存在岗位文件夹的「过程文件」里，并出现在这里。"
              />
            ) : (
              <ul className="rw-file-list">
                {data.recentFiles.map((entry) => (
                  <FileRow
                    key={entry.relativePath}
                    entry={entry}
                    loading={openPath.isPending && openPath.variables === entry.relativePath}
                    onOpen={() => {
                      openPath.mutate(entry.relativePath);
                    }}
                  />
                ))}
              </ul>
            )}
            {openPath.error === null || openPath.variables === JOURNAL_FILE ? null : (
              <ErrorBanner error={openPath.error} />
            )}
          </Panel>
        </div>
      </div>

      {rebindOpen ? (
        <RebindHarnessDialog
          role={role}
          view={data.view}
          harnesses={harnessesQuery.data ?? []}
          activeRuns={activeRunsQuery.data ?? []}
          isRunsPending={activeRunsQuery.isPending}
          isSubmitting={rebindRole.isPending}
          error={rebindRole.error}
          onClose={() => {
            setRebindOpen(false);
          }}
          onConfirm={(harnessId, mode) => {
            rebindRole.mutate(
              { roleId, harnessId, mode },
              {
                onSuccess: () => {
                  setRebindOpen(false);
                },
              },
            );
          }}
        />
      ) : null}
    </div>
  );
}

interface RecentRunsProps {
  teamId: string;
  runs: Run[];
  onOpen: (taskId: string) => void;
}

/** 「最近执行」：执行次数、Harness、状态与开始时间，点行进任务详情。 */
function RecentRuns({ runs, onOpen }: RecentRunsProps) {
  const now = Date.now();
  return (
    <Panel className="rw-panel">
      <h2 className="rw-panel__title">最近执行</h2>
      {runs.length === 0 ? (
        <EmptyState
          bare
          title="暂无执行"
          description="任务派给这个岗位后，每次执行都会出现在这里。"
        />
      ) : (
        <ul className="rw-run-list">
          {runs.map((run) => {
            // 与任务详情页共用同一个展示口径：状态名取后端的 statusLabel（人工接管为「已被接管」）。
            const status = runDisplayStatus(run);
            return (
              <li key={run.id}>
                <button
                  type="button"
                  className="rw-run-row"
                  onClick={() => {
                    onOpen(run.taskId);
                  }}
                >
                  <span className="rw-run-row__key">
                    第 {String(run.attempt)} 次 · {run.taskKey}
                  </span>
                  <span className="rw-run-row__title" title={run.taskKey}>
                    {run.harness.name}
                  </span>
                  <Tag tone={status.tone}>{status.label}</Tag>
                  <span className="rw-run-row__time">{formatRelative(run.startedAt, now)}</span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {/* 说明只在有执行记录时出现：空状态下讲「点击进入任务详情」会让用户
          找不到可点的东西。 */}
      {runs.length === 0 ? null : (
        <p className="rw-note">点击进入任务详情，可查看这次执行的完整进度与改动。</p>
      )}
    </Panel>
  );
}

interface RecentTasksProps {
  teamId: string;
  tasks: TaskSummary[];
  taskCount: number;
  statusLabel: Map<TaskStatus, string>;
}

/** 「最近任务」：编号、标题、状态与更新时间，点行进任务详情。 */
function RecentTasks({ teamId, tasks, taskCount, statusLabel }: RecentTasksProps) {
  const navigate = useNavigate();
  const now = Date.now();

  return (
    <Panel className="rw-panel">
      <div className="rw-panel__head">
        <h2 className="rw-panel__title">最近任务</h2>
        <Button
          variant="neutral"
          onClick={() => {
            void navigate(`/teams/${teamId}/tasks`);
          }}
        >
          查看全部
        </Button>
      </div>

      {tasks.length === 0 ? (
        <EmptyState
          bare
          title="暂无任务"
          description={
            taskCount === 0
              ? "该岗位还没有任务，绑定 Harness 后才能派工。"
              : `该岗位共 ${String(taskCount)} 项任务。`
          }
        />
      ) : (
        <>
          {taskCount > tasks.length ? (
            <p className="rw-note">
              共 {String(taskCount)} 项任务，这里是最近更新的 {String(tasks.length)} 项。
            </p>
          ) : null}
          <ul className="rw-task-list">
            {tasks.map((task) => (
              <li key={task.id}>
                <button
                  type="button"
                  className="rw-task-row"
                  onClick={() => {
                    void navigate(`/teams/${teamId}/tasks/${task.id}`);
                  }}
                >
                  <span className="rw-task-row__key">{task.key}</span>
                  <span className="rw-task-row__title">{task.title}</span>
                  <Tag tone={statusTone[task.status]}>
                    {statusLabel.get(task.status) ?? task.status}
                  </Tag>
                  <span className="rw-task-row__time">{formatRelative(task.updatedAt, now)}</span>
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
    </Panel>
  );
}

interface BindingBodyProps {
  harness: HarnessInstance;
  onChange: () => void;
  /** 打开「更换 Harness」流程弹窗。 */
  onRebind: () => void;
}

function BindingBody({ harness, onChange, onRebind }: BindingBodyProps) {
  const now = Date.now();

  return (
    <>
      <p className="rw-binding__name">
        <span className="rw-binding__label">{harness.name}</span>
        <Tag tone={isTestHarness(harness.preset) ? "warning" : "info"}>
          {presetTagLabel(harness.preset)}
        </Tag>
      </p>
      <dl className="rw-facts">
        <div className="rw-fact">
          <dt className="rw-fact__key">握手状态</dt>
          <dd className="rw-fact__value">
            <Tag tone={healthTone(harness.handshake.status)}>
              {healthLabel(harness.handshake.status)}
            </Tag>
          </dd>
        </div>
        <div className="rw-fact">
          <dt className="rw-fact__key">Agent 版本</dt>
          <dd className="rw-fact__value">{agentVersion(harness.handshake) ?? "—"}</dd>
        </div>
        <div className="rw-fact">
          <dt className="rw-fact__key">最近检查</dt>
          <dd className="rw-fact__value">{formatRelative(harness.handshake.checkedAt, now)}</dd>
        </div>
      </dl>
      {harness.handshake.detail === null ? null : (
        <p className={`rw-note rw-note--${handshakeDetailTone(harness.handshake.status)}`}>
          {harness.handshake.detail}
        </p>
      )}
      <div className="rw-actions">
        <Button variant="neutral" onClick={onRebind}>
          更换 Harness
        </Button>
        <Button variant="subtle" onClick={onChange}>
          换绑 Harness
        </Button>
      </div>
      <p className="rw-note">
        「更换 Harness」按执行情况选择换绑方式，进行中的任务完成后生效；「换绑
        Harness」进入「岗位与绑定」直接改选实例。
      </p>
    </>
  );
}

interface FileRowProps {
  entry: RoleFileEntry;
  loading: boolean;
  onOpen: () => void;
}

/** 「最近文件」的一行：文件名与修改时间，点击在 Finder 中定位。 */
function FileRow({ entry, loading, onOpen }: FileRowProps) {
  const name = entry.relativePath.split("/").at(-1) ?? entry.relativePath;
  return (
    <li>
      <button
        type="button"
        className="rw-file-row"
        title={entry.relativePath}
        onClick={onOpen}
        disabled={loading}
      >
        {loading ? <Spinner /> : null}
        <span className="rw-file-row__name">{name}</span>
        <span className="rw-file-row__time">{formatLocalDateTime(entry.modifiedAt)}</span>
      </button>
    </li>
  );
}
