// 待我处理页：按类别分组列出需要 Human 判断的事项，右侧给出处理入口。
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate, useParams } from "react-router";

import { listRoles } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  EmptyState,
  ErrorBanner,
  PageHeader,
  Panel,
  Spinner,
  Tag,
} from "../../components/index.js";
import type { InboxItem } from "../../bindings/InboxItem.js";
import type { PermissionPrompt } from "../../bindings/PermissionPrompt.js";
import { formatTime } from "../tasks/taskUi.js";
import { DecisionDialog } from "./DecisionDialog.js";
import { DecisionRecorded } from "./DecisionRecorded.js";
import {
  groupInboxItems,
  inboxActionLabel,
  inboxItemTone,
  inboxTargetPath,
  inboxTotal,
  type InboxGroup,
} from "./inboxUi.js";
import { PermissionDecisionCard } from "./PermissionDecisionCard.js";
import { useInbox, useNotifyRefresh } from "./useInbox.js";
import { usePermissionDecision } from "./usePermissionDecision.js";
import "./inbox.css";

/** 可以记录决策的类别：受阻任务与需要拍板的消息。 */
function canRecordDecision(item: InboxItem): boolean {
  return item.category === "blocked" || item.category === "decision";
}

/** 待我处理页。 */
export function InboxPage() {
  const { teamId = "" } = useParams();
  const navigate = useNavigate();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [decisionItem, setDecisionItem] = useState<InboxItem | null>(null);
  const [recorded, setRecorded] = useState<{
    text: string;
    taskKey: string | null;
    roleName: string | null;
    resumedTask: boolean;
  } | null>(null);

  const inbox = useInbox(teamId);
  // 订阅通知事件：收到通知就刷新；轮询兜底已在 useInbox 里。
  useNotifyRefresh(teamId);

  const roles = useQuery({
    queryKey: queryKeys.roles(teamId),
    queryFn: () => listRoles(teamId),
    enabled: teamId !== "",
  });
  const actor = roles.data?.find((view) => view.role.code === "owner")?.role.name ?? "你（Human）";

  const items = inbox.data?.items ?? [];
  const groups = groupInboxItems(items);
  const total = inboxTotal(inbox.data?.counts, items);
  const selected = items.find((item) => item.id === selectedId) ?? items[0] ?? null;

  // 权限类事项就地答复：取这条事项对应的执行记录里的待确认请求。
  const decision = usePermissionDecision(
    teamId,
    selected !== null && selected.category === "permission" ? selected.runId : null,
  );
  // 发起请求的岗位名：执行记录里有 roleId，岗位列表里有名字。
  const roleNameOf = (roleId: string | null): string => {
    if (roleId === null) {
      return "未知岗位";
    }
    return roles.data?.find((view) => view.role.id === roleId)?.role.name ?? "未知岗位";
  };

  return (
    <>
      <PageHeader
        title="待我处理"
        description={`${String(total)} 项需要你的判断，团队其余工作继续推进。`}
        actions={
          <Button variant="neutral" onClick={() => void inbox.refetch()} loading={inbox.isFetching}>
            刷新
          </Button>
        }
      />
      {inbox.isError ? (
        <ErrorBanner
          error={inbox.error}
          action={
            <Button variant="neutral" onClick={() => void inbox.refetch()}>
              重试
            </Button>
          }
        />
      ) : null}

      {recorded !== null ? (
        <DecisionRecorded
          text={recorded.text}
          taskKey={recorded.taskKey}
          roleName={recorded.roleName}
          resumedTask={recorded.resumedTask}
          actor={actor}
          onBack={() => {
            setRecorded(null);
          }}
        />
      ) : items.length === 0 && !inbox.isLoading ? (
        <EmptyState
          title="没有待你处理的事项"
          description="团队其余工作继续推进；有需要你判断的事会出现在这里。"
        />
      ) : (
        <div className="inbox">
          <div className="inbox__list">
            {groups.map((group) => (
              <InboxGroupBlock
                key={group.category}
                group={group}
                selectedId={selected?.id ?? null}
                onSelect={setSelectedId}
                onJump={(item) => {
                  void navigate(inboxTargetPath(teamId, item));
                }}
                onDecide={setDecisionItem}
              />
            ))}
          </div>
          <div className="inbox__detail">
            {selected === null ? (
              <Panel className="inbox__detail-empty">
                <EmptyState bare title="选一条事项查看详情" />
              </Panel>
            ) : (
              <InboxDetail
                item={selected}
                roleName={roleNameOf(decision.run?.roleId ?? null)}
                onJump={() => {
                  void navigate(inboxTargetPath(teamId, selected));
                }}
                onDecide={() => {
                  setDecisionItem(selected);
                }}
                permission={{
                  loading: decision.loading,
                  submitting: decision.submitting,
                  error: decision.error,
                  prompt: decision.prompt,
                  onRespond: decision.respond,
                }}
              />
            )}
          </div>
        </div>
      )}

      {decisionItem === null ? null : (
        <DecisionDialog
          teamId={teamId}
          item={decisionItem}
          onClose={() => {
            setDecisionItem(null);
          }}
          onRecorded={(decision) => {
            setDecisionItem(null);
            setRecorded(decision);
          }}
        />
      )}
    </>
  );
}

interface InboxGroupBlockProps {
  group: InboxGroup;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onJump: (item: InboxItem) => void;
  onDecide: (item: InboxItem) => void;
}

/** 一组同类事项。 */
function InboxGroupBlock({ group, selectedId, onSelect, onJump, onDecide }: InboxGroupBlockProps) {
  return (
    <section className="inbox__group">
      <h2 className="inbox__group-title">
        {group.label}
        <span className="inbox__group-count">{group.items.length}</span>
      </h2>
      <ul className="inbox__items">
        {group.items.map((item) => (
          <li
            key={item.id}
            className={[
              "inbox__item",
              item.id === selectedId ? "inbox__item--active" : "",
              inboxItemTone(item.category) === "warn" ? "inbox__item--warn" : "",
            ]
              .filter((name) => name !== "")
              .join(" ")}
          >
            <button
              type="button"
              className="inbox__item-main"
              onClick={() => {
                onSelect(item.id);
              }}
              aria-pressed={item.id === selectedId}
            >
              <span className="inbox__item-title">{item.title}</span>
              <span className="inbox__item-meta">
                {item.taskKey === null ? null : (
                  <span className="inbox__item-meta-key">{item.taskKey}</span>
                )}
                <span className="inbox__item-meta-label">{item.categoryLabel}</span>
              </span>
              <span className="inbox__item-impact">{item.impact}</span>
              <time className="inbox__item-time" dateTime={item.occurredAt}>
                {formatTime(item.occurredAt)}
              </time>
            </button>
            <div className="inbox__item-actions">
              <Button
                variant="neutral"
                onClick={() => {
                  onJump(item);
                }}
              >
                {inboxActionLabel(item)}
              </Button>
              {canRecordDecision(item) ? (
                <Button
                  variant="subtle"
                  onClick={() => {
                    onDecide(item);
                  }}
                >
                  记录决策
                </Button>
              ) : null}
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}

interface InboxDetailProps {
  item: InboxItem;
  /** 发起请求的岗位名（权限类事项用）。 */
  roleName: string;
  onJump: () => void;
  onDecide: () => void;
  /** 待答复权限的就地答复状态；非权限类事项不用。 */
  permission: {
    loading: boolean;
    submitting: boolean;
    error: unknown;
    prompt: PermissionPrompt | null;
    onRespond: (optionId: string) => void;
  };
}

/** 右侧详情：影响说明、建议操作与处理入口。 */
function InboxDetail({ item, roleName, onJump, onDecide, permission }: InboxDetailProps) {
  const isPermission = item.category === "permission";
  return (
    <Panel className="inbox__detail-panel">
      <div className="inbox__detail-head">
        <Tag tone={item.category === "blocked" ? "warning" : "info"}>{item.categoryLabel}</Tag>
        {item.taskKey === null ? null : <span className="inbox__detail-key">{item.taskKey}</span>}
      </div>
      <h2 className="inbox__detail-title">{item.title}</h2>
      <p className="inbox__detail-impact">{item.impact}</p>
      {isPermission ? null : <p className="inbox__detail-suggestion">{item.suggestion}</p>}
      {item.noChanges ? (
        <p className="inbox__detail-warn">本次执行没有任何改动，Agent 可能提前结束。</p>
      ) : null}
      <time className="inbox__detail-time" dateTime={item.occurredAt}>
        {formatTime(item.occurredAt)}
      </time>

      {isPermission ? (
        permission.loading ? (
          <Spinner label="正在取这次权限请求" />
        ) : permission.prompt === null ? (
          <p className="inbox__detail-suggestion">这个权限请求已经处理完了，或执行已经结束。</p>
        ) : (
          <PermissionDecisionCard
            prompt={permission.prompt}
            roleName={roleName}
            taskKey={item.taskKey}
            submitting={permission.submitting}
            error={permission.error}
            onRespond={permission.onRespond}
          />
        )
      ) : null}

      <div className="inbox__detail-actions">
        {isPermission ? (
          <Button variant="neutral" onClick={onJump}>
            查看任务详情
          </Button>
        ) : (
          <>
            <Button variant="primary" onClick={onJump}>
              {inboxActionLabel(item)}
            </Button>
            {canRecordDecision(item) ? (
              <Button variant="neutral" onClick={onDecide}>
                记录决策
              </Button>
            ) : null}
          </>
        )}
      </div>
    </Panel>
  );
}
