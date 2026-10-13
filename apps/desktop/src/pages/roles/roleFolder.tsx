import { useMutation, useQuery } from "@tanstack/react-query";

import { getRoleFolder, revealRoleFolder } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import type { RoleView } from "../../bindings/RoleView.js";
import { Button, Card, ErrorBanner, Spinner } from "../../components/index.js";

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

export interface RoleFolderSectionProps {
  teamId: string;
  view: RoleView;
}

/** 岗位详情里的「岗位文件夹」区块：相对路径、打开按钮、文件夹问题说明与最近工作日志。 */
export function RoleFolderSection({ teamId, view }: RoleFolderSectionProps) {
  const roleId = view.role.id;
  const folder = useQuery({
    queryKey: roleFolderKey(teamId, roleId),
    queryFn: () => getRoleFolder(roleId, JOURNAL_LIMIT),
  });
  const reveal = useRevealRoleFolder();

  const data = folder.data;
  const relative = data?.relativePath ?? view.folder;
  const absolute = data?.path ?? null;
  // 岗位列表带回的同步失败（view.folderProblem）与本次查询读到的失败（data.problem）
  // 是两个来源，都可能是中文说明，去重后一起显示。
  const problems = [...new Set([view.folderProblem, data?.problem ?? null])].filter(
    (item): item is string => item !== null,
  );
  const journal = data?.journal ?? [];
  const headingId = `role-folder-heading-${roleId}`;

  return (
    <section className="role-detail__folder" aria-labelledby={headingId}>
      <h3 className="role-detail__folder-title" id={headingId}>
        岗位文件夹
      </h3>

      {problems.length === 0 ? null : (
        <Card tone="warning" title="岗位文件夹不可用">
          {problems.map((problem) => (
            <p key={problem} className="role-detail__folder-problem">
              {problem}
            </p>
          ))}
        </Card>
      )}

      <div className="role-detail__folder-body">
        {folder.isPending ? (
          <Spinner label="正在读取岗位文件夹" />
        ) : folder.isError ? (
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
        ) : (
          <>
            <p className="role-detail__folder-path">
              <span className="nowrap">路径：</span>
              <span>{relative ?? "尚未建立"}</span>
            </p>
            {absolute === null ? null : (
              <p className="role-detail__folder-abs" title={absolute}>
                {absolute}
              </p>
            )}

            <div className="role-detail__folder-actions">
              {/* 文件夹不存在时仍然可点：内核会说明具体原因，比一个点不动的按钮更有用。 */}
              <Button
                variant="neutral"
                loading={reveal.isPending}
                onClick={() => {
                  reveal.mutate(roleId);
                }}
              >
                在 Finder 中打开
              </Button>
              {data?.exists === false ? (
                <span className="role-detail__folder-state">文件夹尚不存在</span>
              ) : null}
            </div>

            {reveal.error === null ? null : <ErrorBanner error={reveal.error} />}

            <h4 className="role-detail__folder-subtitle">工作日志</h4>
            {journal.length === 0 ? (
              <p className="role-detail__folder-empty">暂无记录</p>
            ) : (
              <>
                <p className="role-detail__folder-note">
                  共 {String(journal.length)} 条，最新的在最前。
                </p>
                <ol className="role-detail__journal">
                  {journal.map((line, index) => {
                    const entry = splitJournalLine(line);
                    return (
                      <li
                        key={`${String(index)}-${entry.time}`}
                        className="role-detail__journal-item"
                      >
                        {entry.time === "" ? null : (
                          <span className="role-detail__journal-time">{entry.time}</span>
                        )}
                        <span className="role-detail__journal-text">{entry.text}</span>
                      </li>
                    );
                  })}
                </ol>
              </>
            )}
          </>
        )}

        <p className="role-detail__folder-hint">
          更换 Harness 时，新的执行者会先阅读这个文件夹，了解岗位进展。
        </p>
      </div>
    </section>
  );
}
