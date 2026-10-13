// 执行进度流：把一次执行的增量条目按 seq 渲染成消息、计划清单、工具调用卡片、
// 权限请求与决定、文件写入与状态变化。所有内容来自 list_run_updates 与 run_updated 事件。
import { useState } from "react";

import { Spinner, Tag } from "../../../components/index.js";
import type { Run } from "../../../bindings/Run.js";
import type { RunUpdate } from "../../../bindings/RunUpdate.js";
import type { RunUpdateBody } from "../../../bindings/RunUpdateBody.js";
import {
  formatLocal,
  messageRoleName,
  planEntryMarker,
  planPriorityLabel,
  runStatusLabel,
  toolKindName,
  toolKindTone,
  toolStatusName,
  toolStatusTone,
} from "./runUi.js";

interface RunProgressStreamProps {
  run: Run;
  updates: RunUpdate[];
  loading: boolean;
  /** 岗位名，用作消息头像与来源。 */
  roleName: string;
}

export function RunProgressStream({ run, updates, loading, roleName }: RunProgressStreamProps) {
  // Teamspace 生成的任务说明是第一条用户消息，正文很长；默认折叠成一行，
  // 让 Agent 的消息、计划与工具调用能在首屏看到。
  const briefId = updates.find(
    (update) => update.body.type === "message" && update.body.role === "user",
  )?.id;

  return (
    <div className="run-stream">
      {updates.length === 0 ? (
        <p className="run-stream__muted">
          {loading ? "正在读取执行进度…" : "还没有进度，Agent 正在准备执行目录与会话。"}
        </p>
      ) : (
        updates.map((update) => (
          <UpdateRow
            key={update.id}
            update={update}
            run={run}
            roleName={roleName}
            isBrief={update.id === briefId}
          />
        ))
      )}
      {loading && updates.length > 0 ? (
        <p className="run-stream__muted">
          <Spinner /> 正在补齐进度…
        </p>
      ) : null}
    </div>
  );
}

interface UpdateRowProps {
  update: RunUpdate;
  run: Run;
  roleName: string;
  /** 是否是 Teamspace 生成的任务说明（第一条用户消息）。 */
  isBrief: boolean;
}

function UpdateRow({ update, run, roleName, isBrief }: UpdateRowProps) {
  const body = update.body;
  switch (body.type) {
    case "message":
      return <MessageRow body={body} roleName={roleName} run={run} isBrief={isBrief} />;
    case "plan":
      return <PlanCard entries={body.entries} />;
    case "tool_call":
      return <ToolCallCard body={body} />;
    case "permission":
      return <PermissionRow body={body} />;
    case "file_write":
      return <FileWriteRow body={body} />;
    case "status":
      return <StatusRow body={body} />;
  }
}

type MessageBody = Extract<RunUpdateBody, { type: "message" }>;

function MessageRow({
  body,
  roleName,
  run,
  isBrief,
}: {
  body: MessageBody;
  roleName: string;
  run: Run;
  isBrief: boolean;
}) {
  if (isBrief) {
    return <TaskBrief body={body} />;
  }
  const who =
    body.role === "agent" ? `${roleName} · ${run.harness.name}` : messageRoleName(body.role);
  return (
    <div className={`run-msg run-msg--${body.role}`}>
      <span className="run-msg__avatar" aria-hidden="true">
        {roleName.slice(0, 1)}
      </span>
      <div className="run-msg__body">
        <p className="run-msg__who">{who}</p>
        <p className="run-msg__text">{body.text}</p>
      </div>
    </div>
  );
}

/** Teamspace 生成的任务说明：默认折叠为一行，点「展开」显示全文。 */
function TaskBrief({ body }: { body: MessageBody }) {
  const [open, setOpen] = useState(false);
  return (
    <section className={`run-brief${open ? " run-brief--open" : ""}`}>
      <div className="run-brief__head">
        <span className="run-brief__title">任务说明（由 Teamspace 生成）</span>
        <button
          type="button"
          className="run-brief__toggle"
          aria-expanded={open}
          onClick={() => {
            setOpen(!open);
          }}
        >
          {open ? "收起" : "展开"}
        </button>
      </div>
      {open ? (
        <p className="run-brief__text">{body.text}</p>
      ) : (
        <p className="run-brief__hint">点「展开」查看完整说明，Agent 的进展在下方。</p>
      )}
    </section>
  );
}

type PlanBody = Extract<RunUpdateBody, { type: "plan" }>;

function PlanCard({ entries }: { entries: PlanBody["entries"] }) {
  const done = entries.filter((entry) => entry.status === "completed").length;
  return (
    <section className="run-plan">
      <header className="run-plan__head">
        <h4 className="run-plan__title">计划</h4>
        <span className="run-plan__count">
          {String(done)} / {String(entries.length)} 已完成
        </span>
      </header>
      <ul className="run-plan__list">
        {entries.map((entry, index) => (
          <li
            key={`${String(index)}-${entry.content}`}
            className={`run-plan__item run-plan__item--${entry.status}`}
          >
            <span className="run-plan__marker" aria-hidden="true">
              {planEntryMarker(entry.status)}
            </span>
            <span className="run-plan__content">{entry.content}</span>
            {entry.status === "in_progress" ? <Tag tone="info">进行中</Tag> : null}
            {planPriorityLabel(entry.priority) === null ? null : (
              <span className="run-plan__priority">优先级 {planPriorityLabel(entry.priority)}</span>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}

type ToolCallBody = Extract<RunUpdateBody, { type: "tool_call" }>;

function ToolCallCard({ body }: { body: ToolCallBody }) {
  const [open, setOpen] = useState(false);
  const status = toolStatusName(body.status);

  return (
    <section className="run-tool">
      <header className="run-tool__head">
        <Tag tone={toolKindTone(body.kind)}>{toolKindName(body.kind)}</Tag>
        <span className="run-tool__title" title={body.title}>
          {body.title}
        </span>
        {status === null ? null : <Tag tone={toolStatusTone(body.status)}>{status}</Tag>}
      </header>
      {body.command === null ? null : (
        <pre className="run-tool__command" title={body.command}>
          {body.command}
        </pre>
      )}
      {body.paths.length === 0 ? null : (
        <ul className="run-tool__paths">
          {body.paths.map((path) => (
            <li key={path} title={path}>
              {path}
            </li>
          ))}
        </ul>
      )}
      {body.diffs.length === 0 ? null : (
        <>
          <button
            type="button"
            className="run-tool__toggle"
            aria-expanded={open}
            onClick={() => {
              setOpen(!open);
            }}
          >
            diff 摘要
            <span className="run-tool__toggle-hint">{open ? "收起 ▲" : "展开 ▼"}</span>
          </button>
          {open ? (
            <ul className="run-tool__diffs">
              {body.diffs.map((diff) => (
                <li key={diff.path} className="run-tool__diff">
                  <span className="run-tool__diff-path" title={diff.path}>
                    {diff.path}
                  </span>
                  <span className="run-tool__diff-stat">
                    <span className="run-tool__diff-add">+{String(diff.added)}</span>{" "}
                    <span className="run-tool__diff-del">-{String(diff.removed)}</span>
                  </span>
                </li>
              ))}
            </ul>
          ) : null}
        </>
      )}
    </section>
  );
}

type PermissionBody = Extract<RunUpdateBody, { type: "permission" }>;

function PermissionRow({ body }: { body: PermissionBody }) {
  const decision = body.decision;
  return (
    <section className="run-permission">
      <header className="run-permission__head">
        <Tag tone="warning">{toolKindName(body.toolKind)}</Tag>
        <span className="run-permission__title" title={body.title}>
          {body.title}
        </span>
        {decision === null ? (
          <Tag tone="warning">等待你确认</Tag>
        ) : decision.kind === "cancelled" ? (
          <Tag tone="neutral">已撤销</Tag>
        ) : decision.kind.startsWith("allow") ? (
          <Tag tone="success">{decision.by === "rule" ? "自动批准" : "你已允许"}</Tag>
        ) : (
          <Tag tone="danger">已拒绝</Tag>
        )}
      </header>
      {body.command === null ? null : (
        <pre className="run-tool__command" title={body.command}>
          {body.command}
        </pre>
      )}
      {body.paths.length === 0 ? null : (
        <ul className="run-tool__paths">
          {body.paths.map((path) => (
            <li key={path} title={path}>
              {path}
            </li>
          ))}
        </ul>
      )}
      <p className="run-permission__reason">{body.reason}</p>
      {decision === null ? null : (
        <p className="run-permission__decision">
          {decision.reason}（{formatLocal(decision.decidedAt)}）
        </p>
      )}
    </section>
  );
}

type FileWriteBody = Extract<RunUpdateBody, { type: "file_write" }>;

function FileWriteRow({ body }: { body: FileWriteBody }) {
  return (
    <div className="run-filewrite">
      <Tag tone={body.allowed ? "success" : "warning"}>{body.allowed ? "已写入" : "已阻止"}</Tag>
      <span className="run-filewrite__path" title={body.path}>
        {body.path}
      </span>
      <span className="run-filewrite__reason">{body.reason}</span>
    </div>
  );
}

type StatusBody = Extract<RunUpdateBody, { type: "status" }>;

function StatusRow({ body }: { body: StatusBody }) {
  return (
    <p className="run-status-row">
      <span className="run-status-row__dot" aria-hidden="true" />
      <span>{body.detail ?? runStatusLabel[body.status]}</span>
    </p>
  );
}
