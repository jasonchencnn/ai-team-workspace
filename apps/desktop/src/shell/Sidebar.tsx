import { NavLink, useLocation, useParams } from "react-router";
import { useInboxCount } from "../pages/inbox/useInbox.js";
import { TeamSwitcher } from "./TeamSwitcher.js";

interface NavEntry {
  label: string;
  /** 相对 /teams/:teamId 的路径；undefined 表示 M1 不实现。 */
  to?: string;
  end?: boolean;
  note?: string;
  /**
   * 额外的高亮匹配：岗位工作台是团队空间下的岗位子页，
   * 侧边导航仍应高亮「团队空间」。
   */
  alsoMatch?: (pathname: string) => boolean;
  /** 角标类型：待我处理显示待处理总数。 */
  badge?: "inbox";
}

/** 岗位工作台：/teams/:teamId/roles/:roleId/workbench */
const WORKBENCH_PATH = /^\/teams\/[^/]+\/roles\/[^/]+\/workbench\/?$/;

const NAV_ENTRIES: NavEntry[] = [
  { label: "团队空间", to: "", end: true, alsoMatch: (path) => WORKBENCH_PATH.test(path) },
  { label: "任务面板", to: "/tasks" },
  { label: "共享文件", to: "/files", end: true },
  // 岗位工作台在 /roles 之下，但按 Figma 属于「团队空间」，
  // 因此这里必须 end 精确匹配，不能让前缀匹配把工作台也点亮。
  { label: "岗位与绑定", to: "/roles", end: true },
  // 活动与审计按 D22 放在「岗位与绑定」之后、「待我处理」之前。
  { label: "活动与审计", to: "/activity", end: true },
  // 岗位通信（M5-03）与待我处理（M5-04）：角标显示待处理总数。
  { label: "岗位通信", to: "/messages", end: true },
  { label: "待我处理", to: "/inbox", end: true, badge: "inbox" },
];

function DisabledItem({ id, label, note }: { id: string; label: string; note: string }) {
  // 可聚焦的禁用项：读屏读出「标签，已禁用」，并用 aria-describedby 关联说明文字。
  return (
    <span
      className="nav-item nav-item--disabled"
      role="link"
      aria-disabled="true"
      aria-describedby={id}
      tabIndex={0}
      title={note}
    >
      <span>{label}</span>
      <span className="nav-item__note" id={id}>
        {note}
      </span>
    </span>
  );
}

export function Sidebar() {
  const { teamId } = useParams();
  const location = useLocation();
  // 待我处理的总数（与待我处理页共用同一份缓存）。
  const inboxCount = useInboxCount(teamId ?? "");

  return (
    <aside className="sidebar">
      <div className="sidebar__brand">
        <span aria-hidden="true">◈</span>
        <span>Teamspace</span>
      </div>
      <TeamSwitcher currentTeamId={teamId} />
      <p className="sidebar__section-label">团队工作空间</p>
      <nav aria-label="主导航" className="sidebar__nav">
        {NAV_ENTRIES.map((entry) => {
          if (entry.to === undefined) {
            return (
              <DisabledItem
                key={entry.label}
                id={`nav-note-${entry.label}`}
                label={entry.label}
                note={entry.note ?? "暂未提供"}
              />
            );
          }
          if (teamId === undefined) {
            return (
              <DisabledItem
                key={entry.label}
                id={`nav-note-${entry.label}`}
                label={entry.label}
                note="先选择团队"
              />
            );
          }
          return (
            <NavLink
              key={entry.label}
              to={`/teams/${teamId}${entry.to}`}
              end={entry.end ?? false}
              className={({ isActive }) =>
                isActive || (entry.alsoMatch?.(location.pathname) ?? false)
                  ? "nav-item nav-item--active"
                  : "nav-item"
              }
            >
              <span className="nav-item__label">{entry.label}</span>
              {entry.badge === "inbox" && inboxCount > 0 ? (
                <span className="nav-item__badge" aria-label={`${String(inboxCount)} 项待处理`}>
                  {inboxCount}
                </span>
              ) : null}
            </NavLink>
          );
        })}
      </nav>
    </aside>
  );
}
