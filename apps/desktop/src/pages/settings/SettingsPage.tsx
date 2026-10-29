// 设置页：团队自动化（D43）与数据备份（M7-04）。
// 导出与恢复都只能由 Human 在应用里发起：Leader 经控制 socket 调用会被内核拒绝并留痕。
import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { useParams } from "react-router";

import {
  exportBackup,
  inspectBackup,
  pickBackupDestDir,
  restoreBackup,
} from "../../api/commands.js";
import { Button, ErrorBanner, InputField, PageHeader, Panel, Tag } from "../../components/index.js";
import type { BackupExportResult } from "../../bindings/BackupExportResult.js";
import type { BackupRestoreResult } from "../../bindings/BackupRestoreResult.js";
import type { BackupSummary } from "../../bindings/BackupSummary.js";
import { HighRiskConfirm } from "../inbox/HighRiskConfirm.js";
import { AutomationPanel } from "./AutomationPanel.js";
import {
  checkBackupDir,
  checkDestDir,
  describeExport,
  formatBytes,
  formatTimestamp,
  normalizeBackupDir,
  restoreScopeRows,
  restoreWarnings,
  restoreWorkspaceRows,
} from "./backupUi.js";
import "./settings.css";

/** 数据备份设置区。 */
export function SettingsPage() {
  const { teamId = "" } = useParams();
  const [destDir, setDestDir] = useState("");
  const [destProblem, setDestProblem] = useState<string | undefined>(undefined);
  const [exported, setExported] = useState<BackupExportResult | null>(null);

  const [backupDir, setBackupDir] = useState("");
  const [backupProblem, setBackupProblem] = useState<string | undefined>(undefined);
  const [summary, setSummary] = useState<BackupSummary | null>(null);
  const [restored, setRestored] = useState<BackupRestoreResult | null>(null);
  const [confirming, setConfirming] = useState(false);

  const exportMutation = useMutation({
    mutationFn: () => exportBackup({ destDir: destDir.trim(), appVersion: null }),
    onSuccess: (result) => {
      setExported(result);
      setRestored(null);
    },
  });

  const inspectMutation = useMutation({
    mutationFn: () => inspectBackup({ backupDir: normalizeBackupDir(backupDir) }),
    onSuccess: (result) => {
      setSummary(result);
      setRestored(null);
    },
  });

  const restoreMutation = useMutation({
    mutationFn: () => restoreBackup({ backupDir: normalizeBackupDir(backupDir) }),
    onSuccess: (result) => {
      setRestored(result);
      setSummary(null);
      setConfirming(false);
    },
    onError: () => {
      setConfirming(false);
    },
  });

  const startExport = () => {
    const problem = checkDestDir(destDir);
    setDestProblem(problem ?? undefined);
    if (problem !== null) {
      return;
    }
    setExported(null);
    setRestored(null);
    exportMutation.mutate();
  };

  const pickDest = () => {
    pickBackupDestDir()
      .then((picked) => {
        if (picked !== null) {
          setDestDir(picked);
          setDestProblem(undefined);
        }
      })
      .catch((error: unknown) => {
        setDestProblem(error instanceof Error ? error.message : String(error));
      });
  };

  const startInspect = () => {
    const problem = checkBackupDir(backupDir);
    setBackupProblem(problem ?? undefined);
    if (problem !== null) {
      return;
    }
    setSummary(null);
    setRestored(null);
    inspectMutation.mutate();
  };

  const startRestore = () => {
    const problem = checkBackupDir(backupDir);
    setBackupProblem(problem ?? undefined);
    if (problem !== null) {
      return;
    }
    if (summary === null) {
      setBackupProblem("先查看这份备份的摘要，确认无误后再恢复。");
      return;
    }
    setConfirming(true);
  };

  return (
    <>
      <PageHeader
        title="设置"
        description="团队自动化与数据备份。这些设置只有你能改，Leader 无权操作。"
      />

      {teamId === "" ? null : <AutomationPanel teamId={teamId} />}

      <Panel className="settings__panel">
        <h2 className="settings__panel-title">数据备份</h2>
        <p className="settings__panel-lead">
          导出会打包数据库的一致性快照与各团队工作空间下的岗位文件夹（岗位说明、工作日志）。
          恢复前会先自动导出一份「恢复前备份」，恢复出问题时可以退回去。
        </p>

        <section className="settings__block">
          <h3 className="settings__block-title">导出备份</h3>
          <div className="settings__row">
            <InputField
              label="备份目标目录"
              placeholder="例如 ~/Documents/Teamspace-Backups"
              value={destDir}
              error={destProblem}
              hint="备份会建在这个目录下（目录不存在会自动创建）。"
              onChange={(event) => {
                setDestDir(event.target.value);
              }}
            />
            <div className="settings__actions">
              <Button variant="neutral" onClick={pickDest}>
                选择目录
              </Button>
              <Button variant="primary" loading={exportMutation.isPending} onClick={startExport}>
                导出备份
              </Button>
            </div>
          </div>
          {exportMutation.isError ? <ErrorBanner error={exportMutation.error} /> : null}
          {exported === null ? null : (
            <div className="settings__result" data-testid="backup-export-result">
              <p className="settings__result-title">
                <Tag tone="success">已导出</Tag>
                {describeExport(exported)}
              </p>
              <dl className="settings__rows">
                <div className="settings__row-item">
                  <dt>备份目录</dt>
                  <dd>{exported.backupDir}</dd>
                </div>
                <div className="settings__row-item">
                  <dt>导出时间</dt>
                  <dd>{formatTimestamp(exported.exportedAt)}</dd>
                </div>
                <div className="settings__row-item">
                  <dt>大小</dt>
                  <dd>{formatBytes(exported.totalBytes)}</dd>
                </div>
                <div className="settings__row-item">
                  <dt>团队与工作空间</dt>
                  <dd>
                    <ul className="settings__list">
                      {exported.teams.map((team) => (
                        <li key={team.teamId}>
                          <strong>{team.teamName}</strong>
                          <span>{team.workspacePath}</span>
                          {team.roleFolders.length === 0 ? null : (
                            <ul className="settings__list settings__list--nested">
                              {team.roleFolders.map((folder) => (
                                <li key={folder.roleId}>
                                  <strong>
                                    {folder.roleName}
                                    {folder.problem === null ? null : (
                                      <Tag tone="warning">未导出</Tag>
                                    )}
                                  </strong>
                                  <span>
                                    {folder.problem ??
                                      `${folder.relativePath}（${String(folder.fileCount)} 个文件）`}
                                  </span>
                                </li>
                              ))}
                            </ul>
                          )}
                        </li>
                      ))}
                    </ul>
                  </dd>
                </div>
              </dl>
            </div>
          )}
        </section>

        <section className="settings__block">
          <h3 className="settings__block-title">从备份恢复</h3>
          <div className="settings__row">
            <InputField
              label="备份目录"
              placeholder="例如 ~/Documents/Teamspace-Backups/teamspace-backup-20261021-190218"
              value={backupDir}
              error={backupProblem}
              hint="填含 manifest.json 的那一层目录。"
              onChange={(event) => {
                setBackupDir(event.target.value);
                setSummary(null);
                setRestored(null);
              }}
            />
            <div className="settings__actions">
              <Button variant="neutral" loading={inspectMutation.isPending} onClick={startInspect}>
                查看备份
              </Button>
              <Button
                variant="primary"
                disabled={summary === null || restoreMutation.isPending}
                onClick={startRestore}
              >
                恢复这份备份
              </Button>
            </div>
          </div>
          {inspectMutation.isError ? <ErrorBanner error={inspectMutation.error} /> : null}
          {restoreMutation.isError ? <ErrorBanner error={restoreMutation.error} /> : null}

          {summary === null ? null : (
            <div className="settings__result" data-testid="backup-summary">
              <p className="settings__result-title">
                <Tag tone="info">备份摘要</Tag>
                这份备份导出于 {formatTimestamp(summary.exportedAt)}，共 {summary.teamCount}{" "}
                个团队。
              </p>
              <ul className="settings__list">
                {summary.teams.map((team) => (
                  <li key={team.teamId}>
                    <strong>{team.teamName}</strong>
                    <span>{team.workspacePath}</span>
                    {team.roleFolders.length === 0 ? null : (
                      <ul className="settings__list settings__list--nested">
                        {team.roleFolders.map((folder) => (
                          <li key={folder.roleId}>
                            <strong>
                              {folder.roleName}
                              {folder.problem === null ? null : <Tag tone="warning">未导出</Tag>}
                            </strong>
                            <span>{folder.problem ?? folder.relativePath}</span>
                          </li>
                        ))}
                      </ul>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {restored === null ? null : (
            <div className="settings__result" data-testid="backup-restore-result">
              <p className="settings__result-title">
                <Tag tone="success">已恢复</Tag>
                数据库已从备份恢复。
              </p>
              <ul className="settings__list">
                {restored.notes.map((note) => (
                  <li key={note}>{note}</li>
                ))}
              </ul>
              {restored.requiresRestart ? (
                <p className="settings__hint">
                  数据库已换新，为让界面、执行与消息状态完全一致，请重启 Teamspace 应用。
                </p>
              ) : null}
              {restored.skippedFolders.length === 0 ? null : (
                <>
                  <p className="settings__hint">没能写回的岗位文件夹：</p>
                  <ul className="settings__list">
                    {restored.skippedFolders.map((skipped) => (
                      <li key={skipped.roleId}>
                        <strong>{skipped.roleName}</strong>
                        <span>{skipped.reason}</span>
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </div>
          )}
        </section>
      </Panel>

      {confirming && summary !== null ? (
        <HighRiskConfirm
          title="确认从备份恢复"
          tone="danger"
          eyebrow="高风险操作"
          source="恢复会用备份里的数据覆盖当前数据。"
          scope={restoreScopeRows(summary)}
          policy={restoreWarnings(summary)}
          note={
            restoreWorkspaceRows(summary).length === 0 ? (
              "这份备份里没有团队工作空间，恢复后只还原数据库。"
            ) : (
              <ul className="settings__list">
                {restoreWorkspaceRows(summary).map((row) => (
                  <li key={row.label}>
                    <strong>{row.label}</strong>
                    <span>{row.value}</span>
                  </li>
                ))}
              </ul>
            )
          }
          confirmLabel="确认恢复"
          cancelLabel="取消"
          confirmLoading={restoreMutation.isPending}
          onConfirm={() => {
            restoreMutation.mutate();
          }}
          onCancel={() => {
            setConfirming(false);
          }}
        />
      ) : null}
    </>
  );
}
