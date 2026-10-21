// 待我处理页里的权限审批卡：就地允许 / 拒绝，不必跳到任务详情。
// 样式沿用高风险审批卡（13-362）：请求动作与范围用琥珀底，权限计算用白底边框。
// 只回传选项 ID，允许与拒绝的范围仍由内核按既有规则判定（只读与写入各按各的来）。
import { Button, ErrorBanner } from "../../components/index.js";
import type { PermissionPrompt } from "../../bindings/PermissionPrompt.js";
import { permissionScopeNote, toolKindName } from "../task-detail/run/runUi.js";
import { permissionActionText, permissionScopeText, pickPermissionOption } from "./inboxUi.js";

export interface PermissionDecisionCardProps {
  /** 权限请求的内容（来自执行记录里的待确认提示）。 */
  prompt: PermissionPrompt;
  /** 发起请求的岗位名。 */
  roleName: string;
  /** 关联任务编号。 */
  taskKey: string | null;
  /** 提交中。 */
  submitting: boolean;
  /** 提交出错。 */
  error: unknown;
  /** 提交选中的选项：只传 optionId，其余判断留给后端。 */
  onRespond: (optionId: string) => void;
}

/** 待我处理页里的权限审批卡。 */
export function PermissionDecisionCard({
  prompt,
  roleName,
  taskKey,
  submitting,
  error,
  onRespond,
}: PermissionDecisionCardProps) {
  const allow = pickPermissionOption(prompt.options, "allow");
  const reject = pickPermissionOption(prompt.options, "reject");
  const scopeNote = permissionScopeNote(prompt.toolKind, prompt.options);

  return (
    <section className="perm-approve">
      <h3 className="perm-approve__title">权限请求待答复</h3>
      <p className="perm-approve__source">
        请求来自：{roleName}
        {taskKey === null ? null : ` · 关联：${taskKey}`}
      </p>

      <section className="highrisk__section highrisk__section--scope">
        <h4 className="highrisk__section-title">请求动作与范围</h4>
        <dl className="highrisk__rows">
          <div className="highrisk__row">
            <dt>动作</dt>
            <dd>
              {permissionActionText(prompt)}
              {prompt.toolKind === null ? null : ` · ${toolKindName(prompt.toolKind)}`}
            </dd>
          </div>
          <div className="highrisk__row">
            <dt>范围</dt>
            <dd>{permissionScopeText(prompt)}</dd>
          </div>
          <div className="highrisk__row">
            <dt>理由</dt>
            <dd>{prompt.reason}</dd>
          </div>
        </dl>
      </section>

      <section className="highrisk__section highrisk__section--policy">
        <h4 className="highrisk__section-title">权限计算</h4>
        <ul className="highrisk__policy-list">
          <li>放行只覆盖 Agent 这次声明的范围；读取与写入各按各的范围算。</li>
          <li>拒绝会让这次操作停在原地，Agent 收到拒绝结果后自行决定下一步。</li>
          <li>无论允许还是拒绝，都会记入团队活动记录。</li>
        </ul>
      </section>

      {scopeNote === null ? null : <p className="perm-approve__note">{scopeNote}</p>}

      {error === null || error === undefined ? null : <ErrorBanner error={error} />}

      {prompt.options.length === 0 ? (
        <p className="perm-approve__note">Agent 没有提供可选项，只能到任务详情里接管这次执行。</p>
      ) : (
        <div className="perm-approve__actions">
          {allow === null ? null : (
            <Button
              variant="primary"
              loading={submitting}
              disabled={submitting}
              onClick={() => {
                onRespond(allow.optionId);
              }}
            >
              允许
            </Button>
          )}
          {reject === null ? null : (
            <Button
              variant="neutral"
              disabled={submitting}
              onClick={() => {
                onRespond(reject.optionId);
              }}
            >
              拒绝
            </Button>
          )}
        </div>
      )}
    </section>
  );
}
