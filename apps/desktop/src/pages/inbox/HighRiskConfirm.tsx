// 高风险操作审批弹窗（统一「会产生合并」这类操作的确认样式）。
// 只改样式与文案：审批规则、授权范围仍由后端把关，这里只把影响范围讲清楚。
import type { ReactNode } from "react";

import { Button, Modal } from "../../components/index.js";
import "./inbox.css";

/** 「请求动作与范围」里的一行。 */
export interface HighRiskScopeRow {
  label: string;
  value: ReactNode;
}

export interface HighRiskConfirmProps {
  /** 弹窗标题（如「确认合并」）。 */
  title: ReactNode;
  /** 标题上方的小标，默认「高风险操作审批」。 */
  eyebrow?: string;
  /** 「请求来自 …」一行。 */
  source?: ReactNode;
  /** 动作与范围。 */
  scope: HighRiskScopeRow[];
  /** 权限计算的说明行。 */
  policy?: string[];
  /** 底部提示。 */
  note?: ReactNode;
  /** 批准按钮文案。 */
  confirmLabel: string;
  /** 拒绝按钮文案；缺省不显示第二个按钮。 */
  cancelLabel?: string;
  /** 标题小标的语气：danger 用于会造成不可逆影响的操作。 */
  tone?: "primary" | "danger";
  confirmLoading?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/** 高风险操作审批弹窗。 */
export function HighRiskConfirm({
  title,
  eyebrow = "高风险操作审批",
  source,
  scope,
  policy,
  note,
  confirmLabel,
  cancelLabel,
  tone = "primary",
  confirmLoading = false,
  onConfirm,
  onCancel,
}: HighRiskConfirmProps) {
  return (
    <Modal
      title={
        <span className="highrisk__title">
          <span
            className={
              tone === "danger"
                ? "highrisk__eyebrow highrisk__eyebrow--danger"
                : "highrisk__eyebrow"
            }
          >
            {eyebrow}
          </span>
          {title}
        </span>
      }
      description={source}
      onClose={onCancel}
      actions={
        <>
          <Button variant="primary" onClick={onConfirm} loading={confirmLoading}>
            {confirmLabel}
          </Button>
          {cancelLabel === undefined ? null : (
            <Button variant="neutral" onClick={onCancel} disabled={confirmLoading}>
              {cancelLabel}
            </Button>
          )}
        </>
      }
    >
      <section className="highrisk__section highrisk__section--scope">
        <h3 className="highrisk__section-title">请求动作与范围</h3>
        <dl className="highrisk__rows">
          {scope.map((row) => (
            <div className="highrisk__row" key={row.label}>
              <dt>{row.label}</dt>
              <dd>{row.value}</dd>
            </div>
          ))}
        </dl>
      </section>
      {policy === undefined || policy.length === 0 ? null : (
        <section className="highrisk__section highrisk__section--policy">
          <h3 className="highrisk__section-title">权限计算</h3>
          <ul className="highrisk__policy-list">
            {policy.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>{" "}
        </section>
      )}
      {note === undefined ? null : <p className="highrisk__note">{note}</p>}
    </Modal>
  );
}
