import { Button, Card, ErrorBanner } from "../../components/index.js";
import type { RoleView } from "../../bindings/RoleView.js";
import { formatLocalDateTime } from "../team-home/formatLocalDateTime.js";
import { pendingBindingSummary } from "./rebindUi.js";

export interface PendingBindingCardProps {
  view: RoleView;
  onCancel: () => void;
  isCancelling: boolean;
  error: Error | null;
  /** 「查看待投递消息」的跳转回调；没有时这个入口不出现。 */
  onShowPendingMessages?: (() => void) | undefined;
}

/**
 * 换绑待生效提示（Figma 6:404）。
 *
 * 待生效期间当前绑定仍是旧实例：这里只说「待生效」并给出撤销入口，
 * 绝不写成已经换好——否则 Human 会以为新 Harness 已经在跑。
 */
export function PendingBindingCard({
  view,
  onCancel,
  isCancelling,
  error,
  onShowPendingMessages,
}: PendingBindingCardProps) {
  const pending = view.pendingBinding;
  if (pending === null) {
    return null;
  }

  const summary = pendingBindingSummary(
    pending,
    view.harness?.name ?? null,
    formatLocalDateTime(pending.requestedAt),
  );

  return (
    <Card title="换绑待生效" tone="warning" className="pending-binding">
      <p className="pending-binding__summary">
        {summary.title}
        {summary.detail}
      </p>
      <p className="pending-binding__note">
        当前绑定仍是 {summary.oldName}；换绑在登记的条件满足后自动生效，生效前不会变成已换。
      </p>
      <p className="pending-binding__note">
        方式：{summary.modeLabel} · 发起于 {summary.requestedAt}
      </p>

      <div className="pending-binding__actions">
        <Button variant="neutral" loading={isCancelling} onClick={onCancel}>
          撤销换绑
        </Button>
        {onShowPendingMessages === undefined ? null : (
          <Button variant="subtle" onClick={onShowPendingMessages}>
            查看待投递消息
          </Button>
        )}
      </div>

      {error === null ? null : <ErrorBanner error={error} />}
    </Card>
  );
}
