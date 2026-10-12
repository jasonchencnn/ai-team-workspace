import { useQuery } from "@tanstack/react-query";
import { useLocation, useParams } from "react-router";
import { listTeams } from "../api/commands.js";
import { queryKeys } from "../api/queryKeys.js";

export function TopBar() {
  const { teamId } = useParams();
  const location = useLocation();
  const teams = useQuery({ queryKey: queryKeys.teams, queryFn: listTeams });
  const teamName =
    teamId === undefined
      ? location.pathname === "/teams/new"
        ? "创建团队"
        : null
      : (teams.data?.find((team) => team.id === teamId)?.name ?? null);

  return (
    <div className="topbar">
      <nav aria-label="面包屑" className="topbar__crumbs">
        <span>工作空间</span>
        {teamName === null ? null : (
          <>
            <span aria-hidden="true">/</span>
            <span>{teamName}</span>
          </>
        )}
      </nav>
      {/* 工作区分支状态由 M1-07 接入真实数据，这里不放假数据。 */}
      <div aria-label="工作区分支状态">分支状态：待接入</div>
    </div>
  );
}
