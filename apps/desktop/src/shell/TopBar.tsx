import { useQuery } from "@tanstack/react-query";
import { useLocation, useParams } from "react-router";
import { getTeamOverview, listTeams } from "../api/commands.js";
import { queryKeys } from "../api/queryKeys.js";

export function TopBar() {
  const { teamId } = useParams();
  const location = useLocation();
  const teams = useQuery({ queryKey: queryKeys.teams, queryFn: listTeams });
  // 与团队空间共用同一份缓存；无当前团队时不请求。
  const overview = useQuery({
    queryKey: queryKeys.teamOverview(teamId ?? ""),
    queryFn: () => getTeamOverview(teamId ?? ""),
    enabled: teamId !== undefined,
  });
  const teamName =
    teamId === undefined
      ? location.pathname === "/teams/new"
        ? "创建团队"
        : null
      : (teams.data?.find((team) => team.id === teamId)?.name ?? null);

  const workspace = overview.data?.workspace ?? null;
  const problem = overview.data?.workspaceProblem ?? null;

  return (
    <div className="topbar">
      <div className="topbar__crumbs" role="group" aria-label="当前位置">
        <span>工作空间</span>
        {teamName === null ? null : (
          <>
            <span aria-hidden="true">/</span>
            <span aria-current="page">{teamName}</span>
          </>
        )}
      </div>
      {teamId === undefined || overview.data === undefined ? null : workspace !== null ? (
        <p className="topbar__branch" title={workspace.path}>
          <span className="topbar__branch-label">工作区</span>
          {workspace.git === null ? null : <span>{workspace.git.branch ?? "（游离 HEAD）"}</span>}
          {workspace.git?.commit == null ? null : (
            <span className="topbar__commit">{workspace.git.commit}</span>
          )}
        </p>
      ) : problem !== null ? (
        <p className="topbar__branch topbar__branch--warn" role="status" title={problem}>
          工作区异常：{problem}
        </p>
      ) : null}
    </div>
  );
}
