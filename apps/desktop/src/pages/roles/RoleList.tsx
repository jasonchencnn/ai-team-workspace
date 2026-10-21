import type { RoleView } from "../../bindings/RoleView.js";
import { Button, EmptyState, ErrorBanner, Panel, Spinner } from "../../components/index.js";
import { formatLocalDateTime } from "../team-home/formatLocalDateTime.js";
import { pendingBindingSummary } from "./rebindUi.js";

export interface RoleListProps {
  roles: RoleView[];
  isPending: boolean;
  error: Error | null;
  onRetry: () => void;
  selectedRoleId: string | null;
  onSelect: (roleId: string) => void;
  /** 未绑定岗位的「绑定执行者」按钮。 */
  onBindRequest: (roleId: string) => void;
  onCreate: () => void;
}

/** 左侧「团队岗位」：每项显示岗位名与绑定的 Harness 名。 */
export function RoleList({
  roles,
  isPending,
  error,
  onRetry,
  selectedRoleId,
  onSelect,
  onBindRequest,
  onCreate,
}: RoleListProps) {
  return (
    <Panel className="role-list">
      <h2 className="role-list__title">团队岗位</h2>

      {isPending ? <Spinner label="正在加载岗位" /> : null}

      {error === null ? null : (
        <ErrorBanner
          error={error}
          action={
            <Button variant="neutral" onClick={onRetry}>
              重试
            </Button>
          }
        />
      )}

      {!isPending && error === null && roles.length === 0 ? (
        <EmptyState title="还没有岗位" description="新增岗位后即可为它绑定执行者。" />
      ) : null}

      <ul className="role-list__items">
        {roles.map((view) => {
          const selected = view.role.id === selectedRoleId;
          const pending =
            view.pendingBinding === null
              ? null
              : pendingBindingSummary(
                  view.pendingBinding,
                  view.harness?.name ?? null,
                  formatLocalDateTime(view.pendingBinding.requestedAt),
                );
          return (
            <li key={view.role.id} className="role-list__item">
              <button
                type="button"
                className={selected ? "role-chip role-chip--active" : "role-chip"}
                aria-current={selected}
                onClick={() => {
                  onSelect(view.role.id);
                }}
              >
                <span className="role-chip__name">{view.role.name}</span>
                <span className="role-chip__separator">/</span>
                <span className="role-chip__harness">
                  {view.harness === null ? "未绑定" : view.harness.name}
                </span>
                {/* 待生效的换绑：标出来但不写成已换，当前绑定仍是旧的。 */}
                {pending === null ? null : (
                  <span className="role-chip__pending" title={`${pending.title}${pending.detail}`}>
                    待生效
                  </span>
                )}
              </button>

              {selected && pending !== null ? (
                <p className="role-list__pending">
                  {pending.title}
                  {pending.detail}
                </p>
              ) : null}

              {selected && view.harness === null ? (
                <p className="role-list__unbound">未绑定 Harness · 暂不派工</p>
              ) : null}

              {!selected && view.harness === null ? (
                <Button
                  variant="neutral"
                  className="role-list__bind"
                  onClick={() => {
                    onBindRequest(view.role.id);
                  }}
                >
                  绑定执行者
                </Button>
              ) : null}
            </li>
          );
        })}
      </ul>

      <p className="role-list__note">
        任务负责人是岗位。
        <br />
        更换执行工具不改变岗位身份。
      </p>

      <Button variant="neutral" onClick={onCreate}>
        新增岗位
      </Button>
    </Panel>
  );
}
