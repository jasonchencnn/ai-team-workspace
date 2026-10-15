// 权限请求弹窗：执行处于 waiting_permission 时自动弹出，也可在执行区重新打开。
// 内容全部来自 run.pendingPermission：按钮是 Agent 提供的选项，提交时只回传 optionId。
import { Button, ErrorBanner, Modal, Tag } from "../../../components/index.js";
import type { PermissionPrompt } from "../../../bindings/PermissionPrompt.js";
import type { Run } from "../../../bindings/Run.js";
import {
  permissionOptionHint,
  permissionOptionLabel,
  permissionOptionVariant,
  permissionScopeNote,
  toolKindName,
  toolKindTone,
} from "./runUi.js";

/** Agent 想做什么：一句话说明这次请求的操作。 */
function intentSentence(prompt: PermissionPrompt): string {
  switch (prompt.toolKind) {
    case "execute":
      return "Agent 想执行一条命令";
    case "edit":
      return "Agent 想写入或修改文件";
    case "read":
      return "Agent 想读取工作空间之外的文件";
    case "search":
      return "Agent 想在工作空间之外搜索";
    case "delete":
      return "Agent 想删除文件";
    case "move":
      return "Agent 想移动或重命名文件";
    case "fetch":
      return "Agent 想访问网络";
    case "switch_mode":
      return "Agent 想切换工作模式";
    default:
      return "Agent 想执行一项需要确认的操作";
  }
}

interface PermissionDialogProps {
  run: Run;
  prompt: PermissionPrompt;
  roleName: string;
  taskKey: string;
  taskTitle: string;
  submitting: boolean;
  error: unknown;
  /** 提交选项：只传 optionId，其余判断留给后端。 */
  onRespond: (optionId: string) => void;
  /** 关闭弹窗（本次请求仍待确认时可在执行区重新打开）。 */
  onClose: () => void;
}

export function PermissionDialog({
  run,
  prompt,
  roleName,
  taskKey,
  taskTitle,
  submitting,
  error,
  onRespond,
  onClose,
}: PermissionDialogProps) {
  const allowAlways = prompt.options.find((option) => option.kind === "allow_always");
  const rejectAlways = prompt.options.find((option) => option.kind === "reject_always");
  const hint = allowAlways === undefined ? null : permissionOptionHint(allowAlways);
  const scopeNote = permissionScopeNote(prompt.toolKind, prompt.options);

  return (
    <Modal
      title={`${roleName} 请求权限`}
      badge={<Tag tone="warning">等待你确认</Tag>}
      description={`${taskKey} ${taskTitle} · 第 ${String(run.attempt)} 次执行 · ${run.harness.name}`}
      onClose={onClose}
      actions={
        <>
          {prompt.options.map((option) => (
            <Button
              key={option.optionId}
              variant={permissionOptionVariant(option)}
              loading={submitting}
              disabled={submitting}
              onClick={() => {
                onRespond(option.optionId);
              }}
            >
              {permissionOptionLabel(option)}
            </Button>
          ))}
          <Button variant="subtle" onClick={onClose} disabled={submitting}>
            稍后处理
          </Button>
        </>
      }
    >
      <section className="perm-request">
        <div className="perm-request__head">
          <Tag tone={toolKindTone(prompt.toolKind)}>{toolKindName(prompt.toolKind)}</Tag>
          <span className="perm-request__intent">{intentSentence(prompt)}</span>
        </div>
        {prompt.command === null ? null : (
          <pre className="perm-request__code" title={prompt.command}>
            {prompt.command}
          </pre>
        )}
        {prompt.paths.length === 0 ? null : (
          <div className="perm-request__paths">
            <p className="perm-request__paths-label">涉及路径</p>
            <ul>
              {prompt.paths.map((path) => (
                <li key={path} title={path}>
                  {path}
                </li>
              ))}
            </ul>
          </div>
        )}
        <dl className="perm-request__facts">
          <div>
            <dt>工作目录</dt>
            <dd title={run.execDir}>
              {run.execDir}（{run.isolation === "worktree" ? "Git worktree" : "原地：普通文件夹"}）
            </dd>
          </div>
          <div>
            <dt>工具类型</dt>
            <dd>
              {prompt.toolKind ?? "未声明"} · {toolKindName(prompt.toolKind)}
            </dd>
          </div>
        </dl>
      </section>

      <section className="perm-reason">
        <h3 className="perm-reason__title">为什么需要你确认</h3>
        <p className="perm-reason__text">{prompt.reason}</p>
      </section>

      <p className="perm-note">
        任务状态：等待你确认。在你确认前，Agent 暂停在这一步，其他步骤不会继续。
      </p>

      {error === null || error === undefined ? null : <ErrorBanner error={error} />}

      {prompt.options.length === 0 ? (
        <p className="perm-note">Agent 没有提供可选项，只能取消或接管这次执行。</p>
      ) : (
        <p className="perm-note">
          {hint === null ? "选项由 Agent 提供，选择后本次执行会立即继续。" : hint}
          {rejectAlways === undefined ? null : "「总是拒绝」可在「更多选项」中选择。"}
        </p>
      )}
      {scopeNote === null ? null : <p className="perm-note perm-note--scope">{scopeNote}</p>}
      {prompt.options.length === 0 ? null : (
        <p className="perm-note perm-note--source">
          {`对应 Agent 提供的选项：${prompt.options.map((option) => option.name).join(" / ")}`}
        </p>
      )}
    </Modal>
  );
}
