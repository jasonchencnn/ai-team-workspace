import { NavLink, useParams } from "react-router";
import { TeamSwitcher } from "./TeamSwitcher.js";

interface NavEntry {
  label: string;
  /** 相对 /teams/:teamId 的路径；undefined 表示 M1 不实现。 */
  to?: string;
  end?: boolean;
  note?: string;
}

const NAV_ENTRIES: NavEntry[] = [
  { label: "团队空间", to: "", end: true },
  { label: "任务面板", to: "/tasks" },
  { label: "共享文件", note: "M3 提供" },
  { label: "岗位与绑定", to: "/roles" },
  { label: "待我处理", note: "M5 提供" },
];

function DisabledItem({ label, note }: { label: string; note: string }) {
  return (
    <span className="nav-item nav-item--disabled" aria-disabled="true" title={note}>
      <span>{label}</span>
      <span className="nav-item__note">{note}</span>
    </span>
  );
}

export function Sidebar() {
  const { teamId } = useParams();

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
              <DisabledItem key={entry.label} label={entry.label} note={entry.note ?? "暂未提供"} />
            );
          }
          if (teamId === undefined) {
            return <DisabledItem key={entry.label} label={entry.label} note="先选择团队" />;
          }
          return (
            <NavLink
              key={entry.label}
              to={`/teams/${teamId}${entry.to}`}
              end={entry.end ?? false}
              className={({ isActive }) => (isActive ? "nav-item nav-item--active" : "nav-item")}
            >
              {entry.label}
            </NavLink>
          );
        })}
      </nav>
    </aside>
  );
}
