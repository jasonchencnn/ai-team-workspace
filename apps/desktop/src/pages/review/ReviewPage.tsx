// 验收页（M3-06）：执行摘要、改动文件、逐文件 diff（Git）、通过 / 退回、合并冲突后的恢复。
// 能否通过、退回、恢复以及原因一律取自 get_review；合并是否成功、是否冲突取自 approve_review 的结果。
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate, useParams } from "react-router";

import {
  approveReview,
  getReview,
  listRoles,
  listTaskStatuses,
  openRunFile,
  rejectReview,
  rerunAfterConflict,
  resolveMergeManually,
} from "../../api/commands.js";
import { errorMessage } from "../../api/errors.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  Card,
  ErrorBanner,
  Modal,
  PageHeader,
  Panel,
  Spinner,
  Tag,
  TextareaField,
} from "../../components/index.js";
import type { ApproveReviewResult } from "../../bindings/ApproveReviewResult.js";
import type { ReviewView } from "../../bindings/ReviewView.js";
import { fileChangeLabel, fileChangeTone } from "../task-detail/run/runUi.js";
import { formatTime } from "../tasks/taskUi.js";
import { DiffCard } from "./DiffCard.js";
import { advancedTasksOf, formatCount, mergeCommand, reviewActionState } from "./reviewUi.js";
import "./review.css";

/** 评审意见的上限（与后端一致，仅用于输入框限制，校验以后端为准）。 */
const COMMENT_MAX = 2000;

type Confirm = "merge" | "resolve" | "rerun" | null;

export function ReviewPage() {
  const { teamId = "", taskId = "" } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [comment, setComment] = useState("");
  const [confirm, setConfirm] = useState<Confirm>(null);
  const [result, setResult] = useState<ApproveReviewResult | null>(null);
  const [openFiles, setOpenFiles] = useState<ReadonlySet<string>>(new Set());
  const [copied, setCopied] = useState<"ok" | "failed" | null>(null);

  const review = useQuery({
    queryKey: queryKeys.review(teamId, taskId),
    queryFn: () => getReview(taskId),
    enabled: teamId !== "" && taskId !== "",
  });
  const roles = useQuery({
    queryKey: queryKeys.roles(teamId),
    queryFn: () => listRoles(teamId),
    enabled: teamId !== "",
  });
  const statuses = useQuery({
    queryKey: queryKeys.taskStatuses,
    queryFn: listTaskStatuses,
    staleTime: Infinity,
  });

  const taskPath = `/teams/${teamId}/tasks/${taskId}`;

  /** 验收操作之后：任务、看板、执行、活动、岗位（工作日志）相关查询全部失效。 */
  const refreshAll = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.tasks(teamId) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.teamOverview(teamId) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.roles(teamId) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.teamEvents(teamId) }),
    ]);
  };

  const approve = useMutation({
    // expectedHead：验收页预览对应的执行分支 head，后端据此确认合并的就是看过的内容。
    mutationFn: (runId: string) =>
      approveReview({ runId, expectedHead: review.data?.headCommit ?? null }),
    onSuccess: async (data) => {
      setConfirm(null);
      setResult(data);
      await refreshAll();
    },
    onError: async () => {
      setConfirm(null);
      await refreshAll();
    },
  });
  const reject = useMutation({
    mutationFn: (input: { runId: string; comment: string }) => rejectReview(input),
    onSuccess: async () => {
      await refreshAll();
      void navigate(taskPath);
    },
    onError: async () => {
      await refreshAll();
    },
  });
  const resolve = useMutation({
    mutationFn: (runId: string) => resolveMergeManually({ runId }),
    onSuccess: async (data) => {
      setConfirm(null);
      setResult(data);
      await refreshAll();
    },
    onError: async () => {
      setConfirm(null);
      await refreshAll();
    },
  });
  const rerun = useMutation({
    mutationFn: (runId: string) => rerunAfterConflict({ runId }),
    onSuccess: async () => {
      setConfirm(null);
      await refreshAll();
      void navigate(taskPath);
    },
    onError: async () => {
      setConfirm(null);
      await refreshAll();
    },
  });
  const openFile = useMutation({
    mutationFn: (input: { runId: string; path: string }) => openRunFile(input.runId, input.path),
  });

  const backButton = (
    <Button
      variant="neutral"
      onClick={() => {
        void navigate(taskPath);
      }}
    >
      返回任务详情
    </Button>
  );

  if (review.isPending) {
    return (
      <>
        <PageHeader title="验收" actions={backButton} />
        <div className="review__state">
          <Spinner label="正在加载验收数据" />
        </div>
      </>
    );
  }
  if (review.isError) {
    return (
      <>
        <PageHeader title="验收" actions={backButton} />
        <ErrorBanner
          error={review.error}
          action={
            <Button
              variant="neutral"
              onClick={() => {
                void review.refetch();
              }}
            >
              重试
            </Button>
          }
        />
      </>
    );
  }

  const view: ReviewView = review.data;
  const { task, run } = view;
  const state = reviewActionState(view, comment);
  const owner = roles.data?.find((item) => item.role.id === task.ownerRoleId)?.role.name ?? null;
  const statusLabel =
    statuses.data?.find((info) => info.status === task.status)?.label ?? task.status;
  const isGit = view.isolation === "worktree";
  const mutationError =
    approve.error ?? reject.error ?? resolve.error ?? rerun.error ?? openFile.error ?? null;
  const command = mergeCommand(view.baseBranch, view.branch);

  const toggleFile = (path: string, forceOpen = false) => {
    setOpenFiles((previous) => {
      const next = new Set(previous);
      if (forceOpen || !next.has(path)) {
        next.add(path);
      } else {
        next.delete(path);
      }
      return next;
    });
  };
  const jumpToDiff = (index: number, path: string) => {
    toggleFile(path, true);
    window.setTimeout(() => {
      document.getElementById(`review-diff-${String(index)}`)?.scrollIntoView({ block: "start" });
    }, 0);
  };

  const copyCommand = async () => {
    if (command === null) {
      return;
    }
    try {
      await navigator.clipboard.writeText(command);
      setCopied("ok");
    } catch {
      setCopied("failed");
    }
  };

  const header = (
    <PageHeader
      title={`验收 ${task.key} · ${task.title}`}
      description={`${task.key}${owner === null ? "" : ` / ${owner}`} / ${statusLabel}`}
      actions={backButton}
    />
  );

  const banners = result === null ? null : <ResultBanner result={result} />;

  if (run === null) {
    return (
      <>
        {header}
        {banners}
        <Panel className="review__empty">
          <h2 className="review__title">没有待验收的执行</h2>
          {view.problems.length > 0 ? (
            <ul className="review__list">
              {view.problems.map((problem) => (
                <li key={problem}>{problem}</li>
              ))}
            </ul>
          ) : null}
        </Panel>
      </>
    );
  }

  const durationText = formatRange(run.startedAt, run.endedAt);

  return (
    <>
      {header}
      {banners}

      <Panel className="review__info" aria-label="执行信息">
        <InfoCell label="验收的执行" value={`第 ${String(run.attempt)} 次执行（当前）`} />
        <InfoCell label="Harness" value={run.harness.name} />
        {isGit ? (
          <>
            <InfoCell label="基线分支" value={view.baseBranch ?? "未记录（当前分支）"} />
            <InfoCell label="执行分支" value={view.branch ?? "—"} />
          </>
        ) : null}
        <InfoCell label="执行目录" value={isGit ? "独立 worktree" : "原地：普通文件夹"} />
        <div className="review__info-cell">
          <span className="review__caption">执行结果</span>
          <span>
            <Tag tone={run.status === "succeeded" ? "success" : "neutral"}>{run.statusLabel}</Tag>
          </span>
        </div>
      </Panel>

      <div className="review">
        <div className="review__main">
          <Panel className="review__panel">
            <div className="review__panel-head">
              <h2 className="review__title">执行摘要</h2>
              <span className="review__caption">
                第 {String(run.attempt)} 次执行 · {durationText}
              </span>
            </div>
            {view.summary === null || view.summary === "" ? (
              <p className="review__muted">这次执行没有留下摘要。</p>
            ) : (
              <p className="review__summary">{view.summary}</p>
            )}
            <div className="review__journal">
              {run.journal.written ? (
                <Tag tone="success">已写入</Tag>
              ) : (
                <Tag tone="warning">未写入</Tag>
              )}
              <p>
                {run.journal.written
                  ? `岗位工作日志：${run.journal.line ?? "已追加本次执行摘要与改动文件"}`
                  : (run.journal.problem ?? "完成条目尚未写入岗位工作日志")}
              </p>
            </div>
          </Panel>

          <Panel className="review__panel">
            <div className="review__panel-head">
              <h2 className="review__title">改动文件</h2>
              <span className="review__caption">{fileSummary(view)}</span>
            </div>
            {view.filesProblem === null ? null : (
              <p className="review__warn" role="note">
                {view.filesProblem}
              </p>
            )}
            {view.files.length === 0 ? (
              <p className="review__muted">本次执行没有改动文件。</p>
            ) : (
              <ul className="review-files">
                {view.files.map((file, index) => (
                  <li key={file.path} className="review-file">
                    <Tag tone={fileChangeTone[file.change]}>{fileChangeLabel[file.change]}</Tag>
                    {isGit ? (
                      <button
                        type="button"
                        className="review-file__path review-file__path--link"
                        onClick={() => {
                          jumpToDiff(index, file.path);
                        }}
                      >
                        {file.path}
                      </button>
                    ) : (
                      <span className="review-file__path">{file.path}</span>
                    )}
                    {file.binary ? <Tag tone="neutral">二进制</Tag> : null}
                    {file.added !== null && file.removed !== null ? (
                      <>
                        <span className="diff-count diff-count--add">
                          +{formatCount(file.added)}
                        </span>
                        <span className="diff-count diff-count--del">
                          −{formatCount(file.removed)}
                        </span>
                      </>
                    ) : null}
                    {isGit ? null : (
                      <Button
                        variant="neutral"
                        aria-label={`打开文件 ${file.path}`}
                        disabled={file.change === "deleted" || openFile.isPending}
                        onClick={() => {
                          openFile.reset();
                          openFile.mutate({ runId: run.id, path: file.path });
                        }}
                      >
                        打开文件
                      </Button>
                    )}
                  </li>
                ))}
              </ul>
            )}
            <p className="review__caption">
              {isGit
                ? "点文件行定位到下方对应的 diff。数据来自执行分支相对基线的改动，状态以后端为准。"
                : "普通文件夹的改动已经发生在原地，不提供 diff 预览；点「打开文件」用系统默认程序查看当前内容。"}
            </p>
          </Panel>

          {isGit ? (
            <Panel className="review__panel">
              <div className="review__panel-head">
                <h2 className="review__title">逐文件 diff</h2>
                <span className="review__caption">
                  执行分支 {view.branch ?? "—"} ↔ 基线 {view.baseBranch ?? "—"}
                </span>
              </div>
              {view.files.length === 0 ? (
                <p className="review__muted">没有可显示的 diff。</p>
              ) : (
                <div className="review__diffs">
                  {view.files.map((file, index) => (
                    <DiffCard
                      key={file.path}
                      runId={run.id}
                      file={file}
                      anchorId={`review-diff-${String(index)}`}
                      open={openFiles.has(file.path)}
                      onToggle={() => {
                        toggleFile(file.path);
                      }}
                    />
                  ))}
                </div>
              )}
            </Panel>
          ) : null}
        </div>

        <div className="review__side">
          {state.mode === "conflict" ? (
            <Panel className="review__panel review__panel--warning">
              <h2 className="review__title">合并冲突</h2>
              <p className="review__text">
                {run.review?.comment ?? task.blockedReason ?? "合并有冲突，已中止合并。"}
              </p>
              {view.conflictFiles.length > 0 ? (
                <>
                  <p className="review__caption">冲突文件</p>
                  <ul className="review__list review__list--mono">
                    {view.conflictFiles.map((file) => (
                      <li key={file}>{file}</li>
                    ))}
                  </ul>
                </>
              ) : null}
              <p className="review__caption">
                基线分支没有改变。请在仓库里手动合并执行分支并解决冲突，再点「已手动合并，标记完成」；
                也可以丢弃这次执行，基于最新基线「重新执行」。
              </p>
              {command === null ? null : (
                <div className="review__command">
                  <code>{command}</code>
                  <Button
                    variant="neutral"
                    onClick={() => {
                      void copyCommand();
                    }}
                  >
                    复制合并命令
                  </Button>
                  {copied === "ok" ? <span className="review__ok">已复制</span> : null}
                  {copied === "failed" ? (
                    <span className="review__caption">无法写入剪贴板，请手动复制上面的命令</span>
                  ) : null}
                </div>
              )}
              <div className="review__actions">
                <Button
                  disabled={!state.resolveEnabled}
                  onClick={() => {
                    setConfirm("resolve");
                  }}
                >
                  已手动合并，标记完成
                </Button>
                <Button
                  variant="neutral"
                  disabled={!state.rerunEnabled}
                  onClick={() => {
                    setConfirm("rerun");
                  }}
                >
                  重新执行
                </Button>
              </div>
              <ProblemList problems={state.problems} />
            </Panel>
          ) : (
            <Panel className="review__panel">
              <h2 className="review__title">验收操作</h2>
              <p className="review__caption">
                只验收当前这次执行（第 {String(run.attempt)} 次）。若执行已被取消或重新开始，
                下面的按钮会失效，并提示刷新。
              </p>
              <div className="review__block">
                <Button
                  disabled={!state.approveEnabled}
                  loading={approve.isPending}
                  onClick={() => {
                    approve.reset();
                    if (view.approveAction === "merge") {
                      setConfirm("merge");
                    } else {
                      approve.mutate(run.id);
                    }
                  }}
                >
                  {state.approveLabel}
                </Button>
                <p className="review__caption">
                  {isGit
                    ? `会弹出确认：把执行分支合并到 ${view.baseBranch ?? "当前分支"}，成功后清理执行目录；不会推送。`
                    : "改动已经发生在原地，通过后任务直接完成。"}
                </p>
              </div>
              <hr className="review__rule" />
              <div className="review__block">
                <TextareaField
                  label="退回意见（必填）"
                  rows={5}
                  maxLength={COMMENT_MAX}
                  value={comment}
                  onChange={(event) => {
                    setComment(event.target.value);
                  }}
                />
                <div className="review__reject-row">
                  <Button
                    variant="neutral"
                    className="review__reject"
                    disabled={!state.rejectEnabled}
                    loading={reject.isPending}
                    onClick={() => {
                      reject.reset();
                      reject.mutate({ runId: run.id, comment: comment.trim() });
                    }}
                  >
                    退回
                  </Button>
                  {state.rejectHint === null ? null : (
                    <span className="review__caption">{state.rejectHint}</span>
                  )}
                </div>
                <p className="review__caption">
                  退回意见会一并发给 Agent，并写入岗位工作日志。退回后在同一个执行目录里续接会话，
                  继续修改并开始第 {String(run.attempt + 1)} 次执行。
                </p>
              </div>
              <ProblemList problems={state.problems} />
            </Panel>
          )}

          {mutationError === null ? null : (
            <p className="review__error" role="alert">
              {errorMessage(mutationError)}
            </p>
          )}

          <Panel className="review__panel">
            <div className="review__panel-head">
              <h2 className="review__title">历次退回意见</h2>
              <span className="review__caption">{String(view.rejections.length)} 次</span>
            </div>
            {view.rejections.length === 0 ? (
              <p className="review__muted">还没有退回记录</p>
            ) : (
              <ul className="review-rejections">
                {view.rejections.map((item) => (
                  <li key={`${item.runId}-${item.rejectedAt}`} className="review-rejection">
                    <div className="review-rejection__row">
                      <strong>第 {String(item.attempt)} 次执行后退回</strong>
                      <span className="review__caption">{formatTime(item.rejectedAt)}</span>
                    </div>
                    <p>{item.comment}</p>
                  </li>
                ))}
              </ul>
            )}
          </Panel>
        </div>
      </div>

      {confirm === "merge" ? (
        <Modal
          title="确认合并"
          description={`${task.key} ${task.title} · 第 ${String(run.attempt)} 次执行`}
          onClose={() => {
            setConfirm(null);
          }}
          actions={
            <>
              <Button
                loading={approve.isPending}
                onClick={() => {
                  approve.mutate(run.id);
                }}
              >
                确认合并
              </Button>
              <Button
                variant="neutral"
                disabled={approve.isPending}
                onClick={() => {
                  setConfirm(null);
                }}
              >
                取消
              </Button>
            </>
          }
        >
          <p>
            将以 --no-ff 把执行分支 <code>{view.branch ?? "—"}</code> 合并进{" "}
            <code>{view.baseBranch ?? "当前分支"}</code>
            。合并成功后会删除执行目录与执行分支，不会推送。
            合并有冲突时会中止合并、基线保持不变，任务转为受阻。
          </p>
        </Modal>
      ) : null}
      {confirm === "resolve" ? (
        <Modal
          title="已手动合并，标记完成"
          description={`${task.key} ${task.title}`}
          onClose={() => {
            setConfirm(null);
          }}
          actions={
            <>
              <Button
                loading={resolve.isPending}
                onClick={() => {
                  resolve.mutate(run.id);
                }}
              >
                确认标记完成
              </Button>
              <Button
                variant="neutral"
                disabled={resolve.isPending}
                onClick={() => {
                  setConfirm(null);
                }}
              >
                取消
              </Button>
            </>
          }
        >
          <p>
            Teamspace 会检查执行分支已经并入基线分支；通过后任务完成，并清理执行目录与执行分支。
            还没有并入时会被拒绝。
          </p>
        </Modal>
      ) : null}
      {confirm === "rerun" ? (
        <Modal
          title="重新执行"
          description={`${task.key} ${task.title}`}
          onClose={() => {
            setConfirm(null);
          }}
          actions={
            <>
              <Button
                loading={rerun.isPending}
                onClick={() => {
                  rerun.mutate(run.id);
                }}
              >
                确认，丢弃旧执行
              </Button>
              <Button
                variant="neutral"
                disabled={rerun.isPending}
                onClick={() => {
                  setConfirm(null);
                }}
              >
                取消
              </Button>
            </>
          }
        >
          <p>
            会删除旧的执行目录与执行分支（包括其中未合并的改动），任务回到 READY；之后在任务详情点
            「开始执行」，基于最新基线新建一次执行。旧分支的名称与提交会记在活动里。
          </p>
        </Modal>
      ) : null}
    </>
  );
}

function InfoCell({ label, value }: { label: string; value: string }) {
  return (
    <div className="review__info-cell">
      <span className="review__caption">{label}</span>
      <span className="review__info-value">{value}</span>
    </div>
  );
}

function ProblemList({ problems }: { problems: string[] }) {
  if (problems.length === 0) {
    return null;
  }
  return (
    <Card title="现在不能操作的原因" tone="warning">
      <ul className="review__list">
        {problems.map((problem) => (
          <li key={problem}>{problem}</li>
        ))}
      </ul>
    </Card>
  );
}

/** 通过 / 恢复之后的结果提示条（不用弹窗，D22）。下游自动就绪的任务由后端提供后再显示。 */
function ResultBanner({ result }: { result: ApproveReviewResult }) {
  const advanced = advancedTasksOf(result);
  const downstream =
    advanced.length === 0 ? null : (
      <p>下游已自动就绪：{advanced.map((item) => `${item.key} ${item.title}`).join("、")}</p>
    );
  const commit = result.mergedCommit === null ? null : result.mergedCommit.slice(0, 7);
  switch (result.outcome) {
    case "merged":
      return (
        <div className="review-banner review-banner--success" role="status">
          <Tag tone="success">合并成功</Tag>
          <div>
            <p>
              {result.task.key} 已合并到 {result.run.baseBranch ?? "基线分支"}
              {commit === null ? "" : `，合并提交 ${commit}`}，任务已完成。
            </p>
            {downstream}
            {result.cleanupProblem === null ? null : (
              <p className="review__warn">清理执行目录或分支时遇到问题：{result.cleanupProblem}</p>
            )}
          </div>
        </div>
      );
    case "completed":
      return (
        <div className="review-banner review-banner--success" role="status">
          <Tag tone="success">已通过</Tag>
          <p>{result.task.key} 已通过验收，任务已完成。</p>
        </div>
      );
    case "conflict":
      return (
        <div className="review-banner review-banner--warning" role="status">
          <Tag tone="warning">合并有冲突</Tag>
          <p>已中止合并，基线分支没有改变；任务已受阻。请按右侧说明处理。</p>
        </div>
      );
    case "blocked":
      return (
        <div className="review-banner review-banner--warning" role="status">
          <Tag tone="warning">无法合并</Tag>
          <p>{result.reason ?? "现在不能合并。"}任务仍保持待验收，处理后可再次通过。</p>
        </div>
      );
  }
}

function fileSummary(view: ReviewView): string {
  const counted = view.files.filter((file) => file.added !== null && file.removed !== null);
  const added = counted.reduce((sum, file) => sum + (file.added ?? 0), 0);
  const removed = counted.reduce((sum, file) => sum + (file.removed ?? 0), 0);
  return counted.length === 0
    ? `${String(view.files.length)} 个文件`
    : `${String(view.files.length)} 个文件 · +${formatCount(added)} −${formatCount(removed)}`;
}

function formatRange(startedAt: string, endedAt: string | null): string {
  const start = formatTime(startedAt);
  if (endedAt === null) {
    return `${start} 起`;
  }
  const minutes = Math.max(
    0,
    Math.round((new Date(endedAt).getTime() - new Date(startedAt).getTime()) / 60000),
  );
  return `${start} – ${formatTime(endedAt)} · 用时 ${String(minutes)} 分钟`;
}
