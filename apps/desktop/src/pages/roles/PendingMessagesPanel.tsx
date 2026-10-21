import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";

import { listHarnesses } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import { Button, ErrorBanner, Panel, Spinner, Tag } from "../../components/index.js";
import { MESSAGE_STATUS_LABELS, MESSAGE_STATUS_TONES } from "../messages/messagesUi.js";
import { pendingDeliveryNote } from "./rebindUi.js";
import { useRolePendingMessages } from "./useRebindData.js";

export interface PendingMessagesPanelProps {
  teamId: string;
  roleId: string;
  /** 只有存在待生效换绑时才有意义；false 时不取数据也不渲染。 */
  enabled: boolean;
}

/**
 * 换绑期间待投递的消息（Figma 46:815）。
 *
 * 岗位身份不变；每条消息保留入队时的绑定快照（`bindingHarnessId`），
 * 换绑生效后按新绑定投递。每条消息都能跳到岗位通信里的原线程。
 */
export function PendingMessagesPanel({ teamId, roleId, enabled }: PendingMessagesPanelProps) {
  const pendingQuery = useRolePendingMessages(teamId, roleId, enabled);
  const harnessesQuery = useQuery({
    queryKey: queryKeys.harnesses(teamId),
    queryFn: () => listHarnesses(teamId),
    enabled: enabled && teamId !== "",
  });

  if (!enabled) {
    return null;
  }

  const harnessNames = new Map(
    (harnessesQuery.data ?? []).map((instance) => [instance.id, instance.name]),
  );
  const items = pendingQuery.data ?? [];

  return (
    <Panel className="pending-messages" id="pending-messages">
      <h2 className="pending-messages__title">换绑期间待投递消息</h2>
      <p className="pending-messages__note">
        岗位身份不变；每条消息保留入队时的绑定快照，并按策略决定后续投递。
      </p>

      {pendingQuery.isPending ? <Spinner label="正在加载待投递消息" /> : null}

      {pendingQuery.error === null ? null : (
        <ErrorBanner
          error={pendingQuery.error}
          action={
            <Button
              variant="neutral"
              onClick={() => {
                void pendingQuery.refetch();
              }}
            >
              重试
            </Button>
          }
        />
      )}

      {!pendingQuery.isPending && pendingQuery.error === null && items.length === 0 ? (
        <p className="pending-messages__empty">没有待投递的消息。</p>
      ) : null}

      {items.length === 0 ? null : (
        <ul className="pending-messages__list">
          {items.map((item) => {
            const bindingName =
              item.message.bindingHarnessId === null
                ? null
                : (harnessNames.get(item.message.bindingHarnessId) ??
                  item.message.bindingHarnessId);
            return (
              <li key={item.message.id} className="pending-messages__item">
                <div className="pending-messages__head">
                  <span className="pending-messages__subject">{item.thread.subject}</span>
                  <Tag tone={MESSAGE_STATUS_TONES[item.message.status]}>
                    {MESSAGE_STATUS_LABELS[item.message.status]}
                  </Tag>
                </div>
                <p className="pending-messages__binding">{pendingDeliveryNote(bindingName)}</p>
                <p className="pending-messages__body">{item.message.body}</p>
                <Link
                  className="pending-messages__link"
                  to={`/teams/${teamId}/messages?thread=${item.thread.id}&role=${roleId}`}
                >
                  查看岗位通信线程
                </Link>
              </li>
            );
          })}
        </ul>
      )}
    </Panel>
  );
}
