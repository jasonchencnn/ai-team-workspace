import { useQuery } from "@tanstack/react-query";
import { useLocation, useParams, useSearchParams } from "react-router";
import { getTeamOverview, listRoles, listTeams } from "../api/commands.js";
import { queryKeys } from "../api/queryKeys.js";
import { NotificationCenter } from "../pages/inbox/NotificationCenter.js";

/** 共享文件页：/teams/:teamId/files，面包屑里多出「共享文件」和当前文件。 */
const FILES_PATH = /^\/teams\/[^/]+\/files\/?$/;

/** 工作区是普通文件夹时，顶栏用文件夹名代替分支；完整路径放在 title 里。 */
function folderName(path: string): string {
  const segments = path.split("/").filter((segment) => segment !== "");
  return segments.at(-1) ?? path;
}

export function TopBar() {
  const { teamId, roleId } = useParams();
  const location = useLocation();
  const [search] = useSearchParams();
  const teams = useQuery({ queryKey: queryKeys.teams, queryFn: listTeams });
  const onFiles = FILES_PATH.test(location.pathname);
  const currentFile = onFiles ? (search.get("path") ?? "") : "";
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

  // 岗位工作台多一级面包屑；与岗位页共用岗位列表缓存，只在工作台路由时请求。
  const roles = useQuery({
    queryKey: queryKeys.roles(teamId ?? ""),
    queryFn: () => listRoles(teamId ?? ""),
    enabled: teamId !== undefined && roleId !== undefined,
  });
  const roleName =
    roleId === undefined
      ? null
      : (roles.data?.find((view) => view.role.id === roleId)?.role.name ?? null);

  const workspace = overview.data?.workspace ?? null;
  const problem = overview.data?.workspaceProblem ?? null;

  return (
    <div className="topbar">
      <div className="topbar__crumbs" role="group" aria-label="当前位置">
        <span>工作空间</span>
        {teamName === null ? null : (
          <>
            <span aria-hidden="true">/</span>
            <span aria-current={roleName === null ? "page" : undefined}>{teamName}</span>
          </>
        )}
        {roleName === null ? null : (
          <>
            <span aria-hidden="true">/</span>
            <span aria-current="page">{roleName}</span>
          </>
        )}
        {onFiles ? (
          <>
            <span aria-hidden="true">/</span>
            <span aria-current={currentFile === "" ? "page" : undefined}>共享文件</span>
          </>
        ) : null}
        {currentFile === "" ? null : (
          <>
            <span aria-hidden="true">/</span>
            <span aria-current="page">{currentFile}</span>
          </>
        )}
      </div>
      <div className="topbar__right">
        {teamId === undefined || overview.data === undefined ? null : workspace !== null ? (
          <p className="topbar__branch" title={workspace.path}>
            <span className="topbar__branch-label">工作区</span>
            {workspace.git === null ? (
              <span className="topbar__folder">{folderName(workspace.path)}</span>
            ) : (
              <>
                <span>{workspace.git.branch ?? "（游离 HEAD）"}</span>
                {workspace.git.commit == null ? null : (
                  <span className="topbar__commit">{workspace.git.commit}</span>
                )}
              </>
            )}
          </p>
        ) : problem !== null ? (
          <p className="topbar__branch topbar__branch--warn" role="status" title={problem}>
            工作区异常：{problem}
          </p>
        ) : null}
        <NotificationCenter />
      </div>
    </div>
  );
}
