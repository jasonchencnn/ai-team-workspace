// 执行区右栏：权限规则摘要、本次改动文件、历次执行记录。
// 权限规则是对内核 policy.rs 的文案说明，括号里的岗位文件夹、任务引用文件与隔离方式都取自真实数据。
import { Tag } from "../../../components/index.js";
import type { FileRef } from "../../../bindings/FileRef.js";
import type { Run } from "../../../bindings/Run.js";
import { Panel } from "../../../components/index.js";
import { fileChangeLabel, fileChangeTone, formatRelativeStart, runStatusTone } from "./runUi.js";

interface PermissionRulesProps {
  run: Run;
  /** 岗位文件夹相对工作空间的路径，如 `Teamspace/剪辑师`。 */
  roleFolder: string | null;
  /** 任务引用文件（相对工作空间）。 */
  fileRefs: FileRef[];
}

function PermissionRules({ run, roleFolder, fileRefs }: PermissionRulesProps) {
  const isWorktree = run.isolation === "worktree";
  return (
    <Panel className="run-side__panel">
      <h3 className="run-side__title">权限规则</h3>

      <div className="run-rule">
        <Tag tone="success">自动批准</Tag>
        <ul className="run-rule__list">
          <li>读取工作目录内的文件与搜索</li>
          {roleFolder === null ? null : <li>写入本岗位文件夹 {roleFolder}/</li>}
          {fileRefs.length === 0 ? null : (
            <li>
              写入任务引用文件
              {fileRefs.length > 2 ? `（${String(fileRefs.length)} 个）` : ""}：
              {fileRefs.map((ref) => ref.path).join("、")}
            </li>
          )}
          {isWorktree ? <li>Git 工作空间内：写入本次 worktree 分支里的任何位置</li> : null}
        </ul>
      </div>

      <div className="run-rule">
        <Tag tone="warning">转交你确认</Tag>
        <ul className="run-rule__list">
          <li>写入其他位置</li>
          <li>执行命令（如 ffmpeg）</li>
          <li>删除、移动文件，访问网络，切换 Agent 工作模式</li>
        </ul>
      </div>

      <p className="run-side__note">
        同一文件夹同时只运行一个执行；本次为「
        {isWorktree ? "Git worktree 分支" : "原地：普通文件夹"}」。
        {isWorktree ? "Git 工作空间会为每次执行创建独立 worktree 分支。" : null}
      </p>
    </Panel>
  );
}

function ChangedFilesPanel({ run }: { run: Run }) {
  return (
    <Panel className="run-side__panel">
      <div className="run-side__head">
        <h3 className="run-side__title">本次改动文件</h3>
        <span className="run-side__count">{String(run.changedFiles.length)} 个</span>
      </div>
      {run.changedFiles.length === 0 ? (
        <p className="run-side__muted">
          {run.endedAt === null ? "执行进行中，结束后统计改动文件。" : "本次执行没有改动文件。"}
        </p>
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
    </Panel>
  );
}

interface RunHistoryProps {
  runs: Run[];
  selectedRunId: string | null;
  onSelect: (runId: string) => void;
}

function RunHistory({ runs, selectedRunId, onSelect }: RunHistoryProps) {
  return (
    <Panel className="run-side__panel">
      <div className="run-side__head">
        <h3 className="run-side__title">历次执行记录</h3>
        <span className="run-side__count">{String(runs.length)} 次</span>
      </div>
      {runs.length === 0 ? (
        <p className="run-side__muted">这个任务还没有执行过。</p>
      ) : (
        <ul className="run-history">
          {runs.map((run) => {
            const selected = run.id === selectedRunId;
            return (
              <li key={run.id}>
                <button
                  type="button"
                  className={`run-history__item${selected ? " run-history__item--selected" : ""}`}
                  aria-current={selected}
                  onClick={() => {
                    onSelect(run.id);
                  }}
                >
                  <div className="run-history__head">
                    <span className="run-history__attempt">第 {String(run.attempt)} 次执行</span>
                    <Tag tone={runStatusTone[run.status]}>{run.statusLabel}</Tag>
                  </div>
                  <p className="run-history__meta">
                    {run.harness.name} · {formatRelativeStart(run.startedAt)} 起
                    {run.takenOver ? " · 人工接管" : run.status === "cancelled" ? " · 已取消" : ""}
                  </p>
                </button>
              </li>
            );
          })}
        </ul>
      )}
      <p className="run-side__note">执行结束后，摘要自动写入负责岗位的工作日志。</p>
    </Panel>
  );
}

interface RunSidePanelProps {
  run: Run;
  runs: Run[];
  selectedRunId: string | null;
  roleFolder: string | null;
  fileRefs: FileRef[];
  onSelectRun: (runId: string) => void;
}

export function RunSidePanel({
  run,
  runs,
  selectedRunId,
  roleFolder,
  fileRefs,
  onSelectRun,
}: RunSidePanelProps) {
  return (
    <div className="run-side">
      <PermissionRules run={run} roleFolder={roleFolder} fileRefs={fileRefs} />
      <ChangedFilesPanel run={run} />
      <RunHistory runs={runs} selectedRunId={selectedRunId} onSelect={onSelectRun} />
    </div>
  );
}
