// 执行结束后的状态区：成功、失败、已中断（可续接 / 不可续接）、已取消与已人工接管。
// 展示的是后端返回的 summary、failure、failureKind、changedFiles 与执行状态；能否重试、
// 能否续接、能否接管全部以后端为准，界面不自己判断业务规则，所有操作都由上层回调发起。
import { Button, Card, Tag } from "../../../components/index.js";
import type { Run } from "../../../bindings/Run.js";
import type { RunUpdate } from "../../../bindings/RunUpdate.js";
import {
  execDirTitle,
  failureAdvice,
  failureKindLabel,
  fileChangeLabel,
  fileChangeTone,
  formatDuration,
  formatLocal,
  lastEventSummary,
  noResumeReason,
  resumeCapabilityText,
  runElapsedSeconds,
} from "./runUi.js";

/** Agent 未经确认改动了权限范围外的文件（快照比对的结果，后端给出清单）。 */
function OutOfScopeAlert({ run }: { run: Run }) {
  return (
    <section className="run-outcome__alert run-outcome__alert--scope" role="alert">
      <h4 className="run-outcome__alert-title">⚠ Agent 未经确认改动了范围外的文件</h4>
      <p className="run-outcome__alert-text">
        这些文件不在本岗位文件夹、不在任务引用的文件里，执行中也没有收到对应的权限请求。
        {run.isolation === "in_place" ? "原地执行不会自动回滚，请检查后处理。" : ""}
      </p>
      <ul className="run-outcome__alert-files">
        {run.outOfScope.map((path) => (
          <li key={path} title={path}>
            {path}
          </li>
        ))}
      </ul>
    </section>
  );
}

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
                <span className="run-file__tags">
                  {run.outOfScope.includes(file.path) ? <Tag tone="warning">范围外</Tag> : null}
                  <Tag tone={fileChangeTone[file.change]}>{fileChangeLabel[file.change]}</Tag>
                </span>
              </div>
            </li>
          ))}
        </ul>
      )}
      {run.changeNote === null ? null : <p className="run-outcome__muted">{run.changeNote}</p>}
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
          {run.isolation === "worktree" ? `Worktree 分支 ${run.branch ?? "—"}` : "原地：普通文件夹"}
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
  /** 这次执行的进度条目：中断面板用它说明「中断前最后一条事件」。 */
  updates: RunUpdate[];
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
  // 其他结局下的范围外改动提示放在面板上方；成功面板自己处理（任务因此受阻，不是待验收）。
  const alert = run.outOfScope.length === 0 ? null : <OutOfScopeAlert run={run} />;
  const panel =
    run.status === "failed" ? (
      <FailureOutcome {...props} />
    ) : run.status === "interrupted" ? (
      <InterruptedOutcome {...props} />
    ) : run.takenOver ? (
      <TakeoverOutcome {...props} />
    ) : (
      <CancelledOutcome {...props} />
    );
  return (
    <>
      {alert}
      {panel}
    </>
  );
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
  // 后端判定 Agent 改了权限范围外的文件：执行完成，但任务受阻，不进入待验收。
  const scopeBlocked = run.outOfScope.length > 0;
  return (
    <section
      className={`run-outcome run-outcome--success${scopeBlocked ? " run-outcome--scope-blocked" : ""}`}
    >
      <header className="run-outcome__head">
        <h3 className="run-outcome__title">
          {scopeBlocked ? "执行完成，但有范围外改动" : "执行完成"}
        </h3>
        {scopeBlocked ? (
          <Tag tone="warning">已完成 · 受阻</Tag>
        ) : (
          <Tag tone="success">已完成 · 待验收</Tag>
        )}
      </header>
      <Subtitle run={run} taskKey={taskKey} taskTitle={taskTitle} />

      {scopeBlocked ? <OutOfScopeAlert run={run} /> : null}

      <Card title="执行摘要">
        <p className="run-outcome__summary">
          {run.summary === null ? "Agent 没有留下摘要。" : run.summary}
        </p>
      </Card>

      <RunFacts run={run} />

      {changedOpen ? <ChangedFiles run={run} showHint /> : null}

      {run.journal.written ? (
        <div className="run-outcome__journal">
          <p className="run-outcome__journal-title">已写入「{roleName}」工作日志</p>
          <p className="run-outcome__journal-line">{run.journal.line}</p>
        </div>
      ) : (
        <div className="run-outcome__journal run-outcome__journal--problem">
          <p className="run-outcome__journal-title">未能写入工作日志</p>
          <p className="run-outcome__journal-line">
            {run.journal.problem ?? "岗位工作日志没有写入这条记录。"}
          </p>
          <p className="run-outcome__journal-note">
            执行本身已经完成；岗位文件夹恢复可写后，这条记录会自动补写。
          </p>
        </div>
      )}

      <div className="run-outcome__actions">
        <Button variant="neutral" onClick={onToggleChanged} aria-expanded={changedOpen}>
          {changedOpen ? "收起改动" : "查看改动"}
        </Button>
        {scopeBlocked ? null : <Button onClick={onGoReview}>去验收</Button>}
      </div>
      <p className="run-outcome__note">
        {scopeBlocked
          ? "任务已转为受阻：请先检查上面的文件，处理后再继续（原因见任务的受阻原因）。"
          : "执行完成不等于验收通过：任务进入「待验收」，由你查看改动后决定通过或退回。"}
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
  updates,
  taskKey,
  taskTitle,
  busy,
  onStartNewRun,
  onResume,
  onMarkFailed,
  onTakeover,
}: RunOutcomeProps) {
  const lastEvent = lastEventSummary(updates);
  return (
    <section className="run-outcome run-outcome--interrupted">
      <header className="run-outcome__head">
        <h3 className="run-outcome__title">执行已中断</h3>
        <Tag tone="warning">{run.statusLabel}</Tag>
      </header>
      <Subtitle run={run} taskKey={taskKey} taskTitle={taskTitle} />

      <section className="run-outcome__alert">
        <h4 className="run-outcome__alert-title">⚠ 应用重启，与 Agent 的会话已丢失</h4>
        <p className="run-outcome__alert-text">
          应用退出或重启时这次执行仍在进行，Agent 进程已不在，任务保持执行中。
          {run.canResume
            ? `${run.harness.name} 支持续接会话，可以接着上次的上下文继续执行。`
            : "这次执行已经无法回到 Agent 进程，需要你决定继续还是结束。"}
        </p>
      </section>

      <dl className="run-outcome__facts run-outcome__info">
        <div>
          <dt>中断时间</dt>
          <dd>{formatLocal(run.endedAt ?? run.startedAt)}</dd>
        </div>
        <div>
          <dt>最后事件</dt>
          <dd>{lastEvent ?? "没有记录到工具调用或消息。"}</dd>
        </div>
        <div>
          <dt>已有改动</dt>
          <dd>
            {String(run.changedFiles.length)} 个文件 · {execDirTitle(run)}
          </dd>
        </div>
        <div>
          <dt>续接能力</dt>
          <dd>
            {run.harness.name} · {resumeCapabilityText(run)}
          </dd>
        </div>
      </dl>

      <div className="run-outcome__actions">
        <Button
          variant="primary"
          onClick={onResume}
          loading={busy}
          disabled={busy || !run.canResume}
          title={run.canResume ? undefined : noResumeReason(run)}
        >
          续接会话
        </Button>
        <Button
          variant={run.canResume ? "neutral" : "primary"}
          onClick={onStartNewRun}
          loading={busy}
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

      <ul className="run-outcome__legend">
        <li>
          「续接会话」：{run.canResume ? resumeCapabilityText(run) : noResumeReason(run)}
          ，接着上次的上下文继续，不新建执行记录。
        </li>
        <li>
          「重新执行」：新建第 {String(run.attempt + 1)}{" "}
          次执行，这次中断的执行记为失败，已产生的改动保留。
        </li>
        <li>「标记为失败」：把任务转为受阻，之后可以重新执行或换绑 Harness。</li>
      </ul>
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
      </div>

      <section className="run-outcome__state">
        <p className="run-outcome__label">接管后任务状态</p>
        <ul className="run-outcome__state-list">
          <li>任务保持「执行中」，由你在执行目录里处理；岗位与 Harness 绑定不变。</li>
          <li>Agent 已停止，待确认的权限请求被撤销，不再写入任何文件。</li>
          <li>
            已有改动原样保留：
            {String(run.changedFiles.length)} 个文件，不自动回滚，也不合并回基线分支。
          </li>
        </ul>
      </section>

      <section className="run-outcome__next">
        <h4 className="run-outcome__next-title">处理完后，返回执行</h4>
        <p className="run-outcome__next-text">
          可以重新执行第 {String(run.attempt + 1)} 次（由「{roleName}
          」继续），也可以直接把任务提交验收。
        </p>
        <div className="run-outcome__actions">
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
      </section>
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
