// 数据备份页面的纯逻辑：路径校验、备份目录归一化、大小与时间格式化、恢复前的提示文案。
// 这里只做「给人看的」判断，真正的备份与恢复由后端执行（见 crates/teamspace-core/src/backup.rs）。
import type { BackupExportResult } from "../../bindings/BackupExportResult.js";
import type { BackupSummary } from "../../bindings/BackupSummary.js";
import type { BackupTeam } from "../../bindings/BackupTeam.js";

/** 备份清单的文件名（用户可能直接把它的路径粘进来）。 */
export const MANIFEST_FILE = "manifest.json";

/** 校验结果：null 表示没问题，否则是给用户看的中文提示。 */
export type PathProblem = string | null;

/**
 * 校验备份目标目录：不能为空。
 *
 * 相对路径、`~` 开头的路径都放行——内核会把它们解析成绝对路径（见 backup.rs 的 resolve_dir）。
 */
export function checkDestDir(path: string): PathProblem {
  if (path.trim() === "") {
    return "请填写备份目标目录：备份会建在这个目录下。";
  }
  return null;
}

/** 校验要恢复的备份目录：不能为空。 */
export function checkBackupDir(path: string): PathProblem {
  if (path.trim() === "") {
    return "请填写备份目录：填含 manifest.json 的那一层。";
  }
  return null;
}

/**
 * 备份目录归一化：用户可能直接粘贴 manifest.json 的路径，这时取它所在的目录。
 *
 * 末尾的斜杠会被去掉，免得拼出来的路径多一道分隔符。
 */
export function normalizeBackupDir(path: string): string {
  const trimmed = path.trim();
  if (trimmed === "") {
    return "";
  }
  const parts = trimmed.split(/[/\\]/);
  const last = parts[parts.length - 1];
  if (last === MANIFEST_FILE) {
    parts.pop();
    return parts.join("/") || "/";
  }
  return trimmed.replace(/[/\\]+$/, "") || "/";
}

/** 字节数转可读大小（B / KB / MB / GB）。 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit] ?? "B"}`;
}

/** UTC RFC3339 时间转本地时间的「年-月-日 时:分」；解析不了就原样返回。 */
export function formatTimestamp(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return iso;
  }
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${String(date.getFullYear())}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** 导出结果里的一句话摘要。 */
export function describeExport(result: BackupExportResult): string {
  return `已导出 ${String(result.teamCount)} 个团队、${String(result.fileCount)} 个文件（${formatBytes(result.totalBytes)}）。`;
}

/** 备份里没能导出的岗位文件夹数量。 */
export function countMissingFolders(teams: BackupTeam[]): number {
  return teams.reduce(
    (total, team) => total + team.roleFolders.filter((folder) => folder.problem !== null).length,
    0,
  );
}

/** 恢复确认弹窗里的「请求动作与范围」：把备份摘要讲成人话。 */
export function restoreScopeRows(summary: BackupSummary): { label: string; value: string }[] {
  return [
    { label: "备份目录", value: summary.backupDir },
    { label: "导出时间", value: formatTimestamp(summary.exportedAt) },
    { label: "应用版本", value: summary.appVersion },
    {
      label: "数据库版本",
      value: `schema ${String(summary.schemaVersion)}${summary.needsMigration ? "（恢复后自动迁移到当前版本）" : ""}`,
    },
    { label: "团队", value: `${String(summary.teamCount)} 个` },
    {
      label: "文件",
      value: `${String(summary.fileCount)} 个，共 ${formatBytes(summary.totalBytes)}`,
    },
  ];
}

/** 恢复确认弹窗里的提示行：先讲清会覆盖什么，再讲有什么退路。 */
export function restoreWarnings(summary: BackupSummary): string[] {
  const lines = [
    "恢复会用备份里的数据覆盖当前数据：团队、岗位、任务、消息与验收记录都会回到备份时的样子。",
  ];
  if (summary.needsMigration) {
    lines.push("备份来自较早的版本，恢复后会自动迁移到当前版本。");
  }
  const missing = countMissingFolders(summary.teams);
  if (missing > 0) {
    lines.push(`备份里有 ${String(missing)} 个岗位文件夹当时没能导出，恢复时会跳过。`);
  }
  const missingWorkspace = summary.teams.filter((team) => team.roleFolders.length > 0).length;
  if (missingWorkspace === 0) {
    lines.push("这份备份里没有团队工作空间，恢复后只还原数据库。");
  }
  lines.push("恢复前会自动导出一份「恢复前备份」，恢复出问题时可以退回去。");
  return lines;
}

/** 恢复确认弹窗里的工作空间清单：恢复会写回哪些目录。 */
export function restoreWorkspaceRows(summary: BackupSummary): { label: string; value: string }[] {
  return summary.teams.map((team) => ({
    label: team.teamName,
    value: `${team.workspacePath}（${String(team.roleFolders.length)} 个岗位文件夹）`,
  }));
}
