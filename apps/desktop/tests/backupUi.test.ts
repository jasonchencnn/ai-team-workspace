// 数据备份页纯逻辑：路径校验、备份目录归一化、大小与时间格式化、恢复前的提示文案。
import { describe, expect, it } from "vitest";

import type { BackupExportResult } from "../src/bindings/BackupExportResult.js";
import type { BackupSummary } from "../src/bindings/BackupSummary.js";
import type { BackupTeam } from "../src/bindings/BackupTeam.js";
import {
  checkBackupDir,
  checkDestDir,
  countMissingFolders,
  describeExport,
  formatBytes,
  formatTimestamp,
  normalizeBackupDir,
  restoreScopeRows,
  restoreWarnings,
  restoreWorkspaceRows,
} from "../src/pages/settings/backupUi.js";

/** 造一个团队（含 n 个岗位文件夹，其中第一个带问题）。 */
function team(partial: Partial<BackupTeam> = {}): BackupTeam {
  return {
    teamId: "t1",
    teamName: "团队一",
    workspacePath: "/tmp/ws",
    roleFolders: [
      {
        roleId: "r1",
        roleName: "前端开发",
        relativePath: "Teamspace/前端开发",
        backupPath: "workspaces/t1/前端开发",
        fileCount: 2,
        totalBytes: 100,
        problem: null,
      },
    ],
    ...partial,
  };
}

/** 造一份备份摘要。 */
function summary(partial: Partial<BackupSummary> = {}): BackupSummary {
  return {
    backupDir: "/tmp/backups/teamspace-backup-20261021-190218",
    formatVersion: 1,
    appVersion: "0.0.0",
    schemaVersion: 15,
    exportedAt: "2026-10-21T11:02:18.000Z",
    teamCount: 1,
    fileCount: 3,
    totalBytes: 2048,
    teams: [team()],
    needsMigration: false,
    databaseFile: "database/teamspace.db",
    validation: {
      databaseReadable: true,
      databaseIntegrity: "ok",
      databaseSchemaVersion: 15,
      schemaMatches: true,
      filesMatch: true,
      skippedFolders: 0,
      restorable: true,
      problems: [],
    },
    ...partial,
  };
}

describe("备份目标目录校验", () => {
  it("空目录给出提示", () => {
    expect(checkDestDir("")).not.toBeNull();
    expect(checkDestDir("   ")).not.toBeNull();
  });

  it("非空目录放行（相对路径与 ~ 都由内核解析）", () => {
    expect(checkDestDir("/tmp/backups")).toBeNull();
    expect(checkDestDir("backups")).toBeNull();
    expect(checkDestDir("~/Documents/备份")).toBeNull();
  });

  it("提示里说的是中文，且指明要填什么", () => {
    expect(checkDestDir("")).toContain("备份目标目录");
  });
});

describe("备份目录校验", () => {
  it("空目录给出提示", () => {
    expect(checkBackupDir("")).not.toBeNull();
    expect(checkBackupDir("  ")).not.toBeNull();
  });

  it("非空目录放行", () => {
    expect(checkBackupDir("/tmp/backups/teamspace-backup-20261021-190218")).toBeNull();
  });
});

describe("备份目录归一化", () => {
  it("粘贴 manifest.json 的路径时取它所在的目录", () => {
    expect(normalizeBackupDir("/tmp/b/teamspace-backup-1/manifest.json")).toBe(
      "/tmp/b/teamspace-backup-1",
    );
  });

  it("普通目录原样保留", () => {
    expect(normalizeBackupDir("/tmp/b/teamspace-backup-1")).toBe("/tmp/b/teamspace-backup-1");
  });

  it("去掉末尾斜杠", () => {
    expect(normalizeBackupDir("/tmp/b/teamspace-backup-1/")).toBe("/tmp/b/teamspace-backup-1");
  });

  it("空输入还是空", () => {
    expect(normalizeBackupDir("   ")).toBe("");
  });
});

describe("大小与时间格式化", () => {
  it("按单位换算", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1024 * 1024 * 3)).toBe("3.0 MB");
  });

  it("负数与非法值按 0 处理", () => {
    expect(formatBytes(-1)).toBe("0 B");
    expect(formatBytes(Number.NaN)).toBe("0 B");
  });

  it("时间转本地格式；解析不了就原样返回", () => {
    expect(formatTimestamp("2026-10-21T11:02:18.000Z")).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
    expect(formatTimestamp("不是时间")).toBe("不是时间");
  });
});

describe("导出结果摘要", () => {
  it("一句话说清导出了什么", () => {
    const result: BackupExportResult = {
      backupDir: "/tmp/b/teamspace-backup-1",
      manifestPath: "/tmp/b/teamspace-backup-1/manifest.json",
      databasePath: "/tmp/b/teamspace-backup-1/database/teamspace.db",
      exportedAt: "2026-10-21T11:02:18.000Z",
      schemaVersion: 15,
      teamCount: 2,
      fileCount: 12,
      totalBytes: 4096,
      teams: [team()],
    };
    expect(describeExport(result)).toContain("2 个团队");
    expect(describeExport(result)).toContain("12 个文件");
  });
});

describe("恢复前的提示", () => {
  it("先讲覆盖范围，再讲退路", () => {
    const lines = restoreWarnings(summary());
    expect(lines[0]).toContain("覆盖当前数据");
    expect(lines.some((line) => line.includes("恢复前备份"))).toBe(true);
  });

  it("备份比当前版本旧时提示会迁移", () => {
    const lines = restoreWarnings(summary({ needsMigration: true, schemaVersion: 12 }));
    expect(lines.some((line) => line.includes("迁移"))).toBe(true);
  });

  it("备份里有没导出的岗位文件夹时说明会跳过", () => {
    const broken = team({
      roleFolders: [
        {
          roleId: "r1",
          roleName: "前端开发",
          relativePath: "Teamspace/前端开发",
          backupPath: "workspaces/t1/前端开发",
          fileCount: 0,
          totalBytes: 0,
          problem: "工作空间不存在",
        },
      ],
    });
    expect(countMissingFolders([broken])).toBe(1);
    const lines = restoreWarnings(summary({ teams: [broken] }));
    expect(lines.some((line) => line.includes("跳过"))).toBe(true);
  });

  it("范围行覆盖备份目录、版本与团队数", () => {
    const rows = restoreScopeRows(summary());
    const labels = rows.map((row) => row.label);
    expect(labels).toContain("备份目录");
    expect(labels).toContain("数据库版本");
    expect(labels).toContain("团队");
    expect(rows.find((row) => row.label === "团队")?.value).toBe("1 个");
  });

  it("工作空间清单按团队列出", () => {
    const rows = restoreWorkspaceRows(summary());
    expect(rows).toHaveLength(1);
    expect(rows[0]?.label).toBe("团队一");
    expect(rows[0]?.value).toContain("/tmp/ws");
  });
});
