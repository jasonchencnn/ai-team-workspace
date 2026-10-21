import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router";

import { getTeamOverview } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import type { AcpPreset } from "../../bindings/AcpPreset.js";
import type { TaskStatus } from "../../bindings/TaskStatus.js";
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
import type { TagTone } from "../../components/index.js";
import { useRevealRoleFolder } from "../roles/roleFolder.js";
import { rebindModeLabel } from "../roles/rebindUi.js";
import { roleWorkbenchPath } from "../roles/rolesNav.js";
import {
  REBIND_POLL_INTERVAL_MS,
  hasPendingBinding,
  useRebindRefresh,
} from "../roles/useRebindData.js";
import { healthLabel, healthTone } from "../harness-detail/harnessFormat.js";
import { LeaderIntegrationCard } from "./LeaderIntegrationCard.js";
import { formatLocalDateTime } from "./formatLocalDateTime.js";
import "./team-home.css";

/** Harness 预设的中文展示名；mock 是测试用执行器，标注出来避免误当成真实执行环境。 */
const HARNESS_LABEL: Record<AcpPreset, string> = {
  claude_code: "Claude Code",
  codex: "Codex",
  minimax_code: "MiniMax Code",
  gemini_cli: "Gemini CLI",
  codebuddy: "CodeBuddy（腾讯）",
  qoder_cli: "Qoder CLI（阿里）",
  grok_build: "Grok Build（xAI）",
  qwen_code: "Qwen Code（阿里）",
  kimi_cli: "Kimi CLI（月之暗面）",
  opencode: "OpenCode",
  custom: "自定义 ACP Agent",
  mock: "Mock（测试用）",
};

const STATUS_TONE: Record<TaskStatus, TagTone> = {
  todo: "neutral",
  ready: "info",
  doing: "info",
  blocked: "warning",
  review: "warning",
  done: "success",
  cancelled: "neutral",
};

/** TODO / READY 等拉丁文标签前补空格，中文标签不加，避免出现「1 个TODO」这类挤在一起的排版。 */
function formatTaskCount(count: number, label: string): string {
  const latin = /^[\x20-\x7E]/.test(label);
  return `${String(count)} 个${latin ? " " : ""}${label}`;
}

export function TeamHomePage() {
  const { teamId } = useParams();
  const navigate = useNavigate();
  const key = teamId ?? "";

  const overview = useQuery({
    queryKey: queryKeys.teamOverview(key),
    queryFn: () => getTeamOverview(key),
    enabled: teamId !== undefined,
    // 有待生效换绑时轮询：席位上的「待生效」要跟着执行结束自动更新。
    refetchInterval: (query) =>
      hasPendingBinding(query.state.data?.roles ?? []) ? REBIND_POLL_INTERVAL_MS : false,
  });

  // 换绑事件（待生效 / 已生效 / 已撤销）到达时刷新席位。
  useRebindRefresh(key);

  const revealFolder = useRevealRoleFolder();

  if (overview.isPending) {
    return <Spinner label="正在加载团队空间" />;
  }
  if (overview.isError) {
    return (
      <ErrorBanner
        error={overview.error}
        action={
          <Button
            variant="neutral"
            onClick={() => {
              void overview.refetch();
            }}
          >
            重试
          </Button>
        }
      />
    );
  }

  const data = overview.data;
  const totalTasks = data.taskCounts.reduce((sum, item) => sum + item.count, 0);
  const activeCounts = data.taskCounts.filter((item) => item.count > 0);
  const statusLabel = new Map(data.taskCounts.map((item) => [item.status, item.label]));
  const goTask = (taskId: string) => {
    void navigate(`/teams/${data.team.id}/tasks/${taskId}`);
  };

  return (
    <div className="th-page">
      <PageHeader
        title="团队空间"
        description={
          data.team.description.trim() === ""
            ? "让每个岗位，共同推进一件事。"
            : `${data.team.description} · 让每个岗位，共同推进一件事。`
        }
      />

      <section className="th-roles" aria-labelledby="th-roles-heading">
        <h2 className="th-roles__heading" id="th-roles-heading">
          岗位
        </h2>
        {data.roles.length === 0 ? (
          <Panel className="th-panel">
            <EmptyState
              bare
              title="还没有岗位"
              description="团队里还没有岗位，添加岗位后才能创建和派发任务。"
              action={
                <Button
                  variant="neutral"
                  onClick={() => {
                    void navigate(`/teams/${data.team.id}/roles`);
                  }}
                >
                  添加岗位
                </Button>
              }
            />
          </Panel>
        ) : (
          data.roles.map((item) => (
            <Card
              key={item.role.id}
              className="th-role th-role--linked"
              title={
                <Link className="th-role__link" to={roleWorkbenchPath(data.team.id, item.role.id)}>
                  {item.role.name}
                </Link>
              }
            >
              <div className="th-stack">
                {item.harness === null ? (
                  <p>未绑定 Harness · 暂不派工</p>
                ) : (
                  <>
                    <p>
                      当前绑定 {HARNESS_LABEL[item.harness.preset]}
                      {item.harness.name === "" ? "" : ` · ${item.harness.name}`}
                    </p>
                    <div className="th-health">
                      <Tag tone={healthTone(item.harness.handshake.status)}>
                        {healthLabel(item.harness.handshake.status)}
                      </Tag>
                      {item.harness.handshake.detail ? (
                        <span className="th-note">{item.harness.handshake.detail}</span>
                      ) : null}
                    </div>
                  </>
                )}
                {item.openTaskCount > 0 ? (
                  <p className="th-note">未结束任务 {item.openTaskCount}</p>
                ) : null}
                {/* 待生效的换绑：当前绑定仍是旧实例，这里只标「待生效」，不写成已经换好。 */}
                {item.pendingBinding === null ? null : (
                  <p className="th-note th-role__pending">
                    待生效：{item.harness?.name ?? "未绑定"} → {item.pendingBinding.harnessName}（
                    {rebindModeLabel(item.pendingBinding.mode)} ·{" "}
                    {formatLocalDateTime(item.pendingBinding.requestedAt)}）
                  </p>
                )}
                {/* 「岗位文件夹」在卡片里是独立操作，叠在整卡链接之上，不跟着跳转到工作台。 */}
                <div className="th-role__folder">
                  <Button
                    variant="subtle"
                    title={item.folder ?? "岗位文件夹尚未建立"}
                    loading={revealFolder.isPending && revealFolder.variables === item.role.id}
                    onClick={() => {
                      revealFolder.mutate(item.role.id);
                    }}
                  >
                    岗位文件夹
                  </Button>
                  {item.folderProblem === null ? null : (
                    <span className="th-role__folder-warn" title={item.folderProblem}>
                      文件夹不可用
                    </span>
                  )}
                </div>
              </div>
            </Card>
          ))
        )}

        {revealFolder.error === null ? null : <ErrorBanner error={revealFolder.error} />}
      </section>

      <div className="th-panels">
        <Panel className="th-panel">
          <h2 className="th-panel__title">推进中的工作</h2>

          {totalTasks === 0 ? (
            <EmptyState
              bare
              title="团队已就绪，创建第一项任务"
              description="将任务分配给岗位，并关联已有需求文件。岗位当前绑定的 Harness 会在满足依赖后领取任务。"
              action={
                <Button
                  onClick={() => {
                    void navigate(`/teams/${data.team.id}/tasks/new`);
                  }}
                >
                  创建任务
                </Button>
              }
            />
          ) : (
            <div className="th-stack">
              <p className="th-note">
                {activeCounts.length === 0
                  ? "暂无进行中的任务"
                  : activeCounts.map((item) => formatTaskCount(item.count, item.label)).join(" / ")}
              </p>
              {data.activeTasks.length === 0 ? (
                <p className="th-note">当前没有执行中、受阻或待验收的任务。</p>
              ) : (
                <ul className="th-task-list">
                  {data.activeTasks.map((task) => (
                    <li key={task.id}>
                      <button
                        type="button"
                        className="th-task-row"
                        onClick={() => {
                          goTask(task.id);
                        }}
                      >
                        <span className="th-task-row__key">{task.key}</span>
                        <span className="th-task-row__title">{task.title}</span>
                        <span className="th-task-row__owner">{task.owner.name}</span>
                        <Tag tone={STATUS_TONE[task.status]}>
                          {statusLabel.get(task.status) ?? task.status}
                        </Tag>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </Panel>

        <Panel className="th-panel">
          <h2 className="th-panel__title">团队共用的上下文</h2>

          <div className="th-stack">
            {data.workspaceProblem !== null ? (
              <Card tone="warning" title="工作区不可用">
                <p>{data.workspaceProblem}</p>
              </Card>
            ) : data.workspace === null ? (
              <p className="th-note">工作区状态未知。</p>
            ) : (
              <div className="th-stack">
                <p className="th-path">
                  <span className="nowrap">路径：</span>
                  <span title={data.workspace.path}>{data.workspace.path}</span>
                </p>
                {data.workspace.git === null ? (
                  <p>普通文件夹</p>
                ) : (
                  <p>
                    Git 仓库：{data.workspace.git.branch ?? "detached HEAD"} @{" "}
                    {data.workspace.git.commit ?? "暂无提交"}
                  </p>
                )}
              </div>
            )}

            <h3 className="th-panel__subtitle">最近活动</h3>
            {data.recentEvents.length === 0 ? (
              <p className="th-note">暂无活动记录。</p>
            ) : (
              <>
                <p className="th-note">共 {String(data.recentEvents.length)} 条，最新的在最前。</p>
                <ul className="th-event-list">
                  {data.recentEvents.map((event) => (
                    <li key={event.id} className="th-event">
                      <span className="th-event__time">{formatLocalDateTime(event.createdAt)}</span>
                      <span className="th-event__summary">{event.summary}</span>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>
        </Panel>
      </div>

      <section className="th-leader" aria-label="接入 Leader">
        <LeaderIntegrationCard teamId={data.team.id} />
      </section>
    </div>
  );
}
