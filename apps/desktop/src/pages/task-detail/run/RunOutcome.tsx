// 执行结束后的状态区：成功、失败、已中断（可续接 / 不可续接）、已取消与已人工接管。
// 展示的是后端返回的 summary、failure、failureKind、changedFiles 与执行状态；能否重试、
// 能否续接、能否接管全部以后端为准，界面不自己判断业务规则，所有操作都由上层回调发起。
import { Button, Card, Tag } from "../../../components/index.js";
import type { Run } from "../../../bindings/Run.js";
import {
  failureAdvice,
  failureKindLabel,
  fileChangeLabel,
  fileChangeTone,
  formatDuration,
  formatLocal,
  runElapsedSeconds,
} from "./runUi.js";

/** 改动文件列表：成功区与失败区共用。 */
function ChangedFiles({ run, showHint }: { run: Run; showHint: boolean }) {
  return (
    <div className="run-outcome__files">
      <div className="run-outcome__subhead">
        <h4>改动文件</h4>
        <span>{String(run.changedFiles.length)} 个</span>
      </div>
      {run.changedFiles.length === 0 ? (
        <p className="run-outcome__muted">本次执行没有改动文件。</p>
      ) : (
        <ul className="run-files">
          {run.changedFiles.map((file) => (
            <li key={file.path} className="run-file">
              <div className="run-file__head">
                <span className="run-file__path" title={file.path}>
                  {file.path}
                </span>
                <Tag tone={fileChangeTone[file.change]}>{fileChangeLabel[file.change]}</Tag>
              </div>
            </li>
          ))}
        </ul>
      )}
      {showHint ? (
        <p className="run-outcome__muted">
          {run.isolation === "worktree"
            ? "改动都在本次执行的 worktree 分支里，验收通过后才会合并回基线分支。"
            : "原地执行直接改在工作空间里，不会自动回滚。"}
        </p>
      ) : null}
    </div>
  );
}

/** 耗时、工具调用、改动文件数、执行目录（成功区的事实条）。 */
function RunFacts({ run }: { run: Run }) {
  return (
    <dl className="run-outcome__facts">
      <div>
        <dt>用时</dt>
        <dd>{formatDuration(runElapsedSeconds(run))}</dd>
      </div>
      <div>
        <dt>工具调用</dt>
        <dd>{String(run.toolCallCount)} 次</dd>
      </div>
      <div>
        <dt>改动文件</dt>
        <dd>{String(run.changedFiles.length)} 个</dd>
      </div>
      <div>
        <dt>执行目录</dt>
        <dd title={run.execDir}>
          {run.isolation === "worktree" ? "Worktree 分支" : "原地：普通文件夹"}
        </dd>
      </div>
    </dl>
  );
}

export interface RunOutcomeProps {
  run: Run;
  roleName: string;
  taskKey: string;
  taskTitle: string;
  /** 「查看改动」是否展开。 */
  changedOpen: boolean;
  onToggleChanged: () => void;
  /** 「去验收」：滚到任务状态操作区。 */
  onGoReview: () => void;
  /** 任何执行操作进行中：禁用按钮，避免重复提交。 */
  busy: boolean;
  onStartNewRun: () => void;
  onResume: () => void;
  onMarkFailed: () => void;
  onTakeover: () => void;
  onRebind: () => void;
  onSubmitReview: () => void;
  onOpenExecDir: () => void;
  execDirBusy: boolean;
  /** 任务当前是否允许流转到「待验收」（人工接管时用）。 */
  canSubmitReview: boolean;
}

export function RunOutcome(props: RunOutcomeProps) {
  const { run } = props;
  if (run.status === "succeeded") {
    return <SuccessOutcome {...props} />;
  }
  if (run.status === "failed") {
    return <FailureOutcome {...props} />;
  }
  if (run.status === "interrupted") {
    return <InterruptedOutcome {...props} />;
  }
  if (run.takenOver) {
    return <TakeoverOutcome {...props} />;
  }
  return <CancelledOutcome {...props} />;
}

function Subtitle({ run, taskKey, taskTitle }: { run: Run; taskKey: string; taskTitle: string }) {
  return (
    <p className="run-outcome__subtitle">
      {taskKey} {taskTitle} · 第 {String(run.attempt)} 次执行 · {run.harness.name}
    </p>
  );
}

function SuccessOutcome({
  run,
  roleName,
  taskKey,
  taskTitle,
  changedOpen,
  onToggleChanged,
  onGoReview,
}: RunOutcomeProps) {
  return (
    <section className="run-outcome run-outcome--success">
      <header className="run-outcome__head">
        <h3 className="run-outcome__title">执行完成</h3>
        <Tag tone="success">已完成 · 待验收</Tag>
      </header>
      <Subtitle run={run} taskKey={taskKey} taskTitle={taskTitle} />

      <Card title="执行摘要">
        <p className="run-outcome__summary">
          {run.summary === null ? "Agent 没有留下摘要。" : run.summary}
        </p>
      </Card>

      <RunFacts run={run} />

      {changedOpen ? <ChangedFiles run={run} showHint /> : null}

      <div className="run-outcome__journal">
        <p className="run-outcome__journal-title">已写入「{roleName}」工作日志</p>
        <p className="run-outcome__journal-line">
          {formatLocal(run.endedAt ?? run.startedAt)} 完成「{run.taskKey}」第 {String(run.attempt)}{" "}
          次执行， 总时长 {formatDuration(runElapsedSeconds(run))}。
        </p>
      </div>

      <div className="run-outcome__actions">
        <Button variant="neutral" onClick={onToggleChanged} aria-expanded={changedOpen}>
          {changedOpen ? "收起改动" : "查看改动"}
        </Button>
        <Button onClick={onGoReview}>去验收</Button>
      </div>
      <p className="run-outcome__note">
        执行完成不等于验收通过：任务进入「待验收」，由你查看改动后决定通过或退回。
      </p>
    </section>
  );
}

function FailureOutcome({
  run,
  taskKey,
  taskTitle,
  busy,
  onStartNewRun,
  onTakeover,
  onRebind,
}: RunOutcomeProps) {
  const kind = run.failureKind;
  const otherKinds = (
    ["needs_auth", "agent_exited", "timeout", "protocol", "refused", "limit", "launch"] as const
  ).filter((item) => item !== kind);

  return (
    <section className="run-outcome run-outcome--failed">
      <header className="run-outcome__head">
        <h3 className="run-outcome__title">第 {String(run.attempt)} 次执行已失败</h3>
        <Tag tone="danger">{run.statusLabel}</Tag>
      </header>
      <p className="run-outcome__subtitle">
        {taskKey} {taskTitle} · 第 {String(run.attempt)} 次执行 · {run.harness.name} ·{" "}
        {formatLocal(run.startedAt)} 开始
        {run.endedAt === null ? "" : `，${formatLocal(run.endedAt)} 失败`}
      </p>

      <div className="run-failure__kinds">
        <p className="run-outcome__label">失败原因</p>
        <div className="run-failure__tags">
          <Tag tone="danger">{kind === null ? "未分类" : `${failureKindLabel[kind]} · 当前`}</Tag>
          {otherKinds.map((item) => (
            <Tag key={item} tone="neutral">
              {failureKindLabel[item]}
            </Tag>
          ))}
        </div>
      </div>

      {kind === null ? (
        run.failure === null ? null : (
          <p className="run-failure__reason run-failure__reason--plain">{run.failure}</p>
        )
      ) : (
        <section className="run-failure__advice">
          <h4 className="run-failure__advice-title">⚠ {failureKindLabel[kind]}</h4>
          {run.failure === null ? null : <p className="run-failure__reason">{run.failure}</p>}
          <p className="run-failure__text">{failureAdvice(kind)}</p>
        </section>
      )}

      {run.changedFiles.length === 0 ? null : (
        <div className="run-failure__kept">
          <div className="run-outcome__subhead">
            <h4>已保留的改动</h4>
            <span>{String(run.changedFiles.length)} 个文件</span>
          </div>
          <p className="run-outcome__muted">
            {run.isolation === "worktree"
              ? "改动保留在本次执行的 worktree 分支里，重新执行时还能接着改。"
              : "原地执行不会自动回滚：以下文件保持失败前的状态。"}
          </p>
          <ul className="run-files">
            {run.changedFiles.map((file) => (
              <li key={file.path} className="run-file">
                <div className="run-file__head">
                  <span className="run-file__path" title={file.path}>
                    {file.path}
                  </span>
                  <Tag tone={fileChangeTone[file.change]}>{fileChangeLabel[file.change]}</Tag>
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}

      <p className="run-outcome__state">任务状态：已受阻，等待你处理。</p>

      <div className="run-outcome__actions">
        <Button onClick={onStartNewRun} loading={busy} disabled={busy}>
          重试
        </Button>
        <Button variant="neutral" onClick={onTakeover} disabled={busy}>
          人工接管
        </Button>
        <Button variant="neutral" onClick={onRebind}>
          换绑 Harness
        </Button>
      </div>
      <p className="run-outcome__note">
        重试会创建第 {String(run.attempt + 1)} 次执行，不会覆盖前 {String(run.attempt)} 次的记录；
        若 Harness 不可用，可换绑或人工接管。
      </p>
    </section>
  );
}

function InterruptedOutcome({
  run,
  taskKey,
  taskTitle,
  busy,
  onStartNewRun,
  onResume,
  onMarkFailed,
  onTakeover,
}: RunOutcomeProps) {
  return (
    <section className="run-outcome run-outcome--interrupted">
      <header className="run-outcome__head">
        <h3 className="run-outcome__title">执行已中断</h3>
        <Tag tone="warning">{run.statusLabel}</Tag>
      </header>
      <Subtitle run={run} taskKey={taskKey} taskTitle={taskTitle} />
      <p className="run-outcome__muted">
        应用退出或重启时这次执行仍在进行，Agent 进程已不在，任务保持执行中。
      </p>
      {run.canResume ? (
        <p className="run-outcome__note">
          {run.harness.name} 支持续接会话，可以接着上次的上下文继续执行。
        </p>
      ) : (
        <p className="run-outcome__note">
          {run.sessionId === null
            ? "执行在会话建立前就中断了，没有可续接的会话，只能重新执行。"
            : `${run.harness.name} 不支持续接会话（loadSession），只能重新执行。`}
        </p>
      )}

      <div className="run-outcome__actions">
        {run.canResume ? (
          <Button variant="primary" onClick={onResume} loading={busy} disabled={busy}>
            续接会话
          </Button>
        ) : null}
        <Button
          variant={run.canResume ? "neutral" : "primary"}
          onClick={onStartNewRun}
          disabled={busy}
        >
          重新执行
        </Button>
        <Button variant="subtle" onClick={onTakeover} disabled={busy}>
          人工接管
        </Button>
        <Button variant="subtle" onClick={onMarkFailed} disabled={busy}>
          标记为失败
        </Button>
      </div>
      <p className="run-outcome__note">
        「标记为失败」会把任务转为受阻，之后可以重新执行或换绑 Harness。
      </p>
    </section>
  );
}

function TakeoverOutcome({
  run,
  roleName,
  taskKey,
  taskTitle,
  busy,
  onStartNewRun,
  onSubmitReview,
  onOpenExecDir,
  execDirBusy,
  canSubmitReview,
}: RunOutcomeProps) {
  return (
    <section className="run-outcome run-outcome--taken-over">
      <header className="run-outcome__head">
        <h3 className="run-outcome__title">已由你接管</h3>
        <Tag tone="info">人工接管</Tag>
      </header>
      <Subtitle run={run} taskKey={taskKey} taskTitle={taskTitle} />
      <p className="run-outcome__muted">
        这条执行已停止，Agent 的写入权限已撤销。下面的内容由你在执行目录里自己处理，Teamspace
        不再自动改动它。
      </p>

      <div className="run-execdir">
        <p className="run-outcome__label">执行目录</p>
        <p className="run-execdir__path" title={run.execDir}>
          {run.execDir}
        </p>
        <p className="run-outcome__muted">
          {run.isolation === "worktree"
            ? `Git worktree 分支 ${run.branch ?? "—"}，改动都在这里。`
            : "原地执行，这个目录就是工作空间本身。"}
        </p>
      </div>

      <div className="run-outcome__actions">
        <Button
          variant="neutral"
          onClick={onOpenExecDir}
          loading={execDirBusy}
          disabled={execDirBusy}
        >
          在 Finder 中打开
        </Button>
        <Button onClick={onStartNewRun} loading={busy} disabled={busy}>
          结束接管并重新执行
        </Button>
        <Button
          variant="subtle"
          onClick={onSubmitReview}
          loading={busy}
          disabled={busy || !canSubmitReview}
          title={canSubmitReview ? undefined : "任务当前不允许流转到「待验收」"}
        >
          直接提交验收
        </Button>
      </div>
      <p className="run-outcome__note">
        处理完成后可以重新执行第 {String(run.attempt + 1)} 次（由「{roleName}
        」继续），也可以直接把任务提交验收。
      </p>
    </section>
  );
}

function CancelledOutcome({
  run,
  taskKey,
  taskTitle,
  busy,
  onStartNewRun,
  onRebind,
}: RunOutcomeProps) {
  return (
    <section className="run-outcome run-outcome--cancelled">
      <header className="run-outcome__head">
        <h3 className="run-outcome__title">执行已取消</h3>
        <Tag tone="neutral">{run.statusLabel}</Tag>
      </header>
      <Subtitle run={run} taskKey={taskKey} taskTitle={taskTitle} />
      {run.failure === null ? null : <p className="run-outcome__muted">{run.failure}</p>}
      <div className="run-outcome__actions">
        <Button onClick={onStartNewRun} loading={busy} disabled={busy}>
          重新执行
        </Button>
        <Button variant="neutral" onClick={onRebind}>
          换绑 Harness
        </Button>
      </div>
      <p className="run-outcome__note">
        取消后任务回到「待开始」，可再次派工；已经产生的改动不会被自动回滚。
      </p>
    </section>
  );
}
