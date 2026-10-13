import { useMutation, useQuery } from "@tanstack/react-query";

import { getRoleFolder, revealRoleFolder } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import type { RoleView } from "../../bindings/RoleView.js";
import { Link } from "react-router";
import { Button, ErrorBanner, Spinner } from "../../components/index.js";
import { roleWorkbenchPath } from "./rolesNav.js";

/** 工作日志取最近 20 条，与内核 `DEFAULT_JOURNAL_LIMIT` 一致。 */
const JOURNAL_LIMIT = 20;

/**
 * 岗位文件夹查询 key 挂在岗位 key 之下，因此岗位编辑、绑定、解绑、归档成功后
 * `useRoleMutations` 失效岗位 key 时会一并刷新这里，无需页面各自补一次刷新。
 */
export function roleFolderKey(teamId: string, roleId: string) {
  return [...queryKeys.roles(teamId), "folder", roleId] as const;
}

/** 在 Finder 中打开岗位文件夹；打开文件夹不改变数据，所以成功后不失效查询。 */
export function useRevealRoleFolder() {
  return useMutation({
    mutationFn: (roleId: string) => revealRoleFolder(roleId),
  });
}

/**
 * 工作日志一行形如 `2026-10-13 14:05:01 UTC+08:00 · FE-001 · 状态 执行中 → 受阻：等待接口`
 * 或 `2026-10-13 14:05:01 UTC+08:00 · 创建岗位`。只按第一个「 · 」切分时间与内容，
 * 内容里再出现的「 · 」原样保留。
 */
export function splitJournalLine(line: string): { time: string; text: string } {
  const at = line.indexOf(" · ");
  if (at < 0) {
    return { time: "", text: line };
  }
  return { time: line.slice(0, at), text: line.slice(at + 3) };
}

export interface RoleFolderLineProps {
  teamId: string;
  view: RoleView;
}

/**
 * 岗位详情里的岗位文件夹入口：只保留一行路径与「打开岗位工作台」。
 * 工作日志、过程文件和文件夹问题的完整视图都在岗位工作台上，这里不再重复。
 */
export function RoleFolderLine({ teamId, view }: RoleFolderLineProps) {
  const roleId = view.role.id;
  const folder = useQuery({
    queryKey: roleFolderKey(teamId, roleId),
    queryFn: () => getRoleFolder(roleId, JOURNAL_LIMIT),
  });

  const relative = folder.data?.relativePath ?? view.folder;
  const problem = folder.data?.problem ?? view.folderProblem;

  return (
    <section className="role-detail__folder" aria-label="岗位文件夹">
      {folder.isPending ? (
        <Spinner label="正在读取岗位文件夹" />
      ) : (
        <>
          <p className="role-detail__folder-path" title={folder.data?.path ?? undefined}>
            <span className="nowrap">路径：</span>
            <span>{relative ?? "尚未建立"}</span>
          </p>
          {problem === null ? null : (
            <p className="role-detail__folder-problem" title={problem}>
              {problem}
            </p>
          )}
        </>
      )}
      <div className="role-detail__folder-actions">
        <Link className="ui-button ui-button--neutral" to={roleWorkbenchPath(teamId, roleId)}>
          打开岗位工作台
        </Link>
      </div>
      {folder.error === null ? null : (
        <ErrorBanner
          error={folder.error}
          action={
            <Button
              variant="neutral"
              onClick={() => {
                void folder.refetch();
              }}
            >
              重试
            </Button>
          }
        />
      )}
    </section>
  );
}
