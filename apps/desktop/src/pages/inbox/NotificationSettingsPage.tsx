// 通知订阅与投递策略：按类别总开关 + 岗位开关，读写后端的通知订阅设置。
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router";

import { getNotificationSettings, updateNotificationSettings } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import { Button, ErrorBanner, PageHeader, Panel, Tag } from "../../components/index.js";
import type { NotificationCategory } from "../../bindings/NotificationCategory.js";
import {
  categoryLabel,
  diffDraft,
  draftChanged,
  draftFromSettings,
  isRoleEnabled,
  NOTIFICATION_CATEGORY_DEFAULT_ROLES,
  NOTIFICATION_CATEGORY_SEVERITY,
  setCategoryEnabled,
  setRoleEnabled,
  type SubscriptionDraft,
} from "./inboxUi.js";
import "./inbox.css";

/** 严重级别 → 标签色。 */
function severityTone(severity: "normal" | "important" | "high") {
  if (severity === "high") {
    return "danger" as const;
  }
  if (severity === "important") {
    return "warning" as const;
  }
  return "neutral" as const;
}

const SEVERITY_LABEL: Record<"normal" | "important" | "high", string> = {
  normal: "普通",
  important: "重要",
  high: "高",
};

/** 通知订阅页。 */
export function NotificationSettingsPage() {
  const { teamId = "" } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState<SubscriptionDraft | null>(null);
  const [saved, setSaved] = useState(false);

  const settings = useQuery({
    queryKey: queryKeys.notificationSettings(teamId),
    queryFn: () => getNotificationSettings({ teamId }),
    enabled: teamId !== "",
  });

  // 后端设置变化时重置草稿（首次加载与保存后的返回值都会走到这里）。
  useEffect(() => {
    if (settings.data !== undefined) {
      setDraft(draftFromSettings(settings.data));
    }
  }, [settings.data]);

  const mutation = useMutation({
    mutationFn: () => {
      if (settings.data === undefined || draft === null) {
        return Promise.reject(new Error("通知订阅设置还没加载"));
      }
      const categories = diffDraft(settings.data, draft);
      return updateNotificationSettings({ teamId, categories });
    },
    onSuccess: (next) => {
      queryClient.setQueryData(queryKeys.notificationSettings(teamId), next);
      setDraft(draftFromSettings(next));
      setSaved(true);
    },
  });

  const categories = settings.data?.categories ?? [];
  const dirty = settings.data !== undefined && draft !== null && draftChanged(settings.data, draft);

  const toggleCategory = (category: NotificationCategory, enabled: boolean) => {
    setSaved(false);
    setDraft((previous) =>
      previous === null ? previous : setCategoryEnabled(previous, category, enabled),
    );
  };

  const toggleRole = (category: NotificationCategory, roleId: string, enabled: boolean) => {
    setSaved(false);
    setDraft((previous) =>
      previous === null ? previous : setRoleEnabled(previous, category, roleId, enabled),
    );
  };

  return (
    <>
      <PageHeader
        title="通知订阅与投递策略"
        description="把系统事件按岗位、严重级别和交付规则做成可追踪的非即时消息。"
        actions={
          <Button
            variant="neutral"
            onClick={() => {
              void navigate(`/teams/${teamId}/messages`);
            }}
            disabled={teamId === ""}
          >
            查看线程审计
          </Button>
        }
      />
      {settings.isError ? <ErrorBanner error={settings.error} /> : null}
      {mutation.isError ? <ErrorBanner error={mutation.error} /> : null}

      <div className="notify-settings__route">
        <Panel className="notify-settings__panel">
          <h2 className="notify-settings__panel-title">默认路由</h2>
          <p className="notify-settings__panel-lead">
            目标始终是指定 Role；系统在投递时才解析该 Role 的当前有效 Binding。
          </p>
          <ul className="notify-settings__routes">
            <li>
              <span>任务进入 REVIEW</span>
              <span className="notify-settings__route-target">Leader、QA</span>
              <Tag tone="success">立即</Tag>
            </li>
            <li>
              <span>任务变为 BLOCKED</span>
              <span className="notify-settings__route-target">Leader、产品</span>
              <Tag tone="success">立即</Tag>
            </li>
            <li>
              <span>高风险操作待审批</span>
              <span className="notify-settings__route-target">Human、Leader</span>
              <Tag tone="warning">需提醒</Tag>
            </li>
          </ul>
        </Panel>
        <Panel className="notify-settings__panel">
          <h2 className="notify-settings__panel-title">投递保障</h2>
          <p className="notify-settings__panel-lead">
            策略只决定「谁收到」，不绕过权限、依赖、租约或人工审批。
          </p>
          <ul className="notify-settings__routes">
            <li>
              <span>去重窗口</span>
              <span className="notify-settings__route-target">
                event + task + target Role，5 分钟内合并
              </span>
            </li>
            <li>
              <span>失败重试</span>
              <span className="notify-settings__route-target">
                指数退避；最多 3 次；每次生成审计 Event
              </span>
            </li>
            <li>
              <span>无有效 Binding</span>
              <span className="notify-settings__route-target">
                保证 MESSAGE_NEEDS_HUMAN；仅 Human / 授权 Role 可改派
              </span>
            </li>
          </ul>
        </Panel>
      </div>

      <Panel className="notify-settings__table-panel">
        <h2 className="notify-settings__panel-title">
          默认通知覆盖
          <span className="notify-settings__count">（{categories.length} 类）</span>
        </h2>
        <table className="notify-settings__table">
          <thead>
            <tr>
              <th scope="col">通知类别</th>
              <th scope="col">按岗位订阅</th>
              <th scope="col">严重级别</th>
              <th scope="col">投递范围</th>
              <th scope="col">状态</th>
            </tr>
          </thead>
          <tbody>
            {categories.map((category) => {
              const severity = NOTIFICATION_CATEGORY_SEVERITY[category.category];
              const masterOn = draft?.categoryEnabled[category.category] ?? category.enabled;
              return (
                <tr key={category.category}>
                  <th scope="row">
                    <span className="notify-settings__category">{categoryLabel(category)}</span>
                    <span className="notify-settings__category-code">{category.category}</span>
                  </th>
                  <td>
                    <div className="notify-settings__roles">
                      {category.roles.length === 0 ? (
                        <span className="notify-settings__no-role">团队里还没有岗位</span>
                      ) : (
                        category.roles.map((role) => {
                          const on =
                            draft === null
                              ? role.enabled
                              : isRoleEnabled(draft, category.category, role.roleId);
                          return (
                            <label className="notify-settings__role" key={role.roleId}>
                              <input
                                type="checkbox"
                                role="switch"
                                checked={on}
                                disabled={!masterOn || mutation.isPending}
                                onChange={(event) => {
                                  toggleRole(category.category, role.roleId, event.target.checked);
                                }}
                              />
                              <span>{role.roleName}</span>
                              <span className="notify-settings__role-code">{role.roleCode}</span>
                            </label>
                          );
                        })
                      )}
                    </div>
                    <p className="notify-settings__scope">
                      {NOTIFICATION_CATEGORY_DEFAULT_ROLES[category.category]}
                    </p>
                  </td>
                  <td>
                    <Tag tone={severityTone(severity)}>{SEVERITY_LABEL[severity]}</Tag>
                  </td>
                  <td className="notify-settings__delivery">当前 Binding；订阅决定是否送达</td>
                  <td>
                    <label className="notify-settings__master">
                      <input
                        type="checkbox"
                        role="switch"
                        checked={masterOn}
                        disabled={mutation.isPending}
                        onChange={(event) => {
                          toggleCategory(category.category, event.target.checked);
                        }}
                      />
                      <span>{masterOn ? "已启用" : "已停用"}</span>
                    </label>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        <footer className="notify-settings__footer">
          <p className="notify-settings__note">
            保存后，仅影响后续事件；已入队消息保留原始 Binding 快照。
          </p>
          {saved ? <p className="notify-settings__saved">策略已保存。</p> : null}
          <div className="notify-settings__actions">
            <Button
              variant="neutral"
              disabled={!dirty || mutation.isPending}
              onClick={() => {
                setSaved(false);
                if (settings.data !== undefined) {
                  setDraft(draftFromSettings(settings.data));
                }
              }}
            >
              取消
            </Button>
            <Button
              variant="primary"
              disabled={!dirty || mutation.isPending}
              loading={mutation.isPending}
              onClick={() => {
                mutation.mutate();
              }}
            >
              保存策略
            </Button>
          </div>
        </footer>
      </Panel>

      <Panel className="notify-settings__explain">
        <h2 className="notify-settings__panel-title">状态变更通知</h2>
        <p className="notify-settings__panel-lead">
          状态先经权限与版本校验成功，再由订阅策略决定是否创建岗位消息。
        </p>
        <section className="notify-settings__block notify-settings__block--event">
          <h3>状态事件</h3>
          <p className="notify-settings__mono">TASK_REVIEW_REQUESTED</p>
          <p>Task: FE-023 · 由 role_frontend 提交</p>
          <p>Artifact: tasks/FE-023-result.md</p>
        </section>
        <section className="notify-settings__block">
          <h3>订阅策略解析</h3>
          <p>Review 请求 → 通知 Leader / role_leader</p>
          <p>BLOCKED → 通知负责人、Leader 与订阅 Human</p>
          <p>高风险审批 → 只通知授权 Human</p>
        </section>
        <section className="notify-settings__block">
          <h3>自动投递链路</h3>
          <p>Event → MESSAGE_SENT → MESSAGE_ROUTED</p>
          <p>→ 解析 role_leader 当前有效 Binding</p>
          <p>→ MESSAGE_DELIVERED → 目标 Adapter</p>
        </section>
        <section className="notify-settings__block notify-settings__block--warn">
          <h3>Binding 变更时的规则</h3>
          <p>旧 Binding 失效后拒绝新消息。</p>
          <p>尚未路由的消息在投递时解析新 Binding；已送达消息保留原 Binding 快照。</p>
          <p>无有效 Binding 时进入 NEEDS_HUMAN，不静默改派。</p>
        </section>
        <p className="notify-settings__note">
          收到消息不等于领取 Task，也不能越过依赖、租约、权限或 Human 审批。重复 Event
          通过幂等键去重，不重复创建消息。
        </p>
      </Panel>
    </>
  );
}
