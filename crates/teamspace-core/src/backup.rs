//! 数据备份与恢复（M7-04）。
//!
//! 备份目录结构：
//!
//! ```text
//! teamspace-backup-20261021-190218/
//!   manifest.json                       清单：版本、schema、导出时间、团队与文件清单
//!   database/teamspace.db               数据库一致性快照（`VACUUM INTO`，不复制正在写的文件）
//!   workspaces/<团队标识>/<岗位文件夹>/…  各团队工作空间下 `Teamspace/` 的岗位文件夹副本
//! ```
//!
//! 恢复的顺序：
//! 1. 校验清单：备份的 schema 比当前程序新时**拒绝**（旧版本则恢复后跑迁移）；
//! 2. 自动导出一份「恢复前备份」，恢复出问题时可以退回去；
//! 3. 恢复数据库（关掉旧连接 → 覆盖主库文件 → 重新打开并迁移）；
//! 4. 岗位文件夹按清单写回原工作空间路径；工作空间不存在时跳过，并在结果里列出。
//!
//! 备份与恢复都只由 Human 在应用里发起（Leader 无权，见 [`crate::dispatch`]）。

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::api::Teamspace;
use crate::error::{Error, Result};
use crate::model::{
    BackupExportResult, BackupFile, BackupManifest, BackupRestoreResult, BackupRoleFolder,
    BackupSummary, BackupTeam, ExportBackupInput, InspectBackupInput, RestoreBackupInput,
    SkippedRoleFolder,
};
use crate::service::backup as backup_svc;
use crate::store;

/// 备份格式版本：清单结构变了就加一（比当前程序新的备份拒绝恢复）。
pub const BACKUP_FORMAT_VERSION: u32 = 1;
/// 清单文件名。
pub const MANIFEST_FILE: &str = "manifest.json";
/// 数据库快照所在子目录。
pub const DATABASE_DIR: &str = "database";
/// 数据库快照文件名。
pub const DATABASE_FILE: &str = "teamspace.db";
/// 岗位文件夹副本所在子目录。
pub const WORKSPACES_DIR: &str = "workspaces";
/// 备份目录名前缀。
pub const BACKUP_PREFIX: &str = "teamspace-backup-";
/// 恢复前自动备份的目录名前缀。
pub const PRE_RESTORE_PREFIX: &str = "teamspace-pre-restore-";
/// 恢复前备份在备份目录旁边的存放子目录（父目录不可写时的兜底）。
pub const PRE_RESTORE_FALLBACK_DIR: &str = "restore-pre-backups";

impl Teamspace {
    /// 导出备份：在 `dest_dir` 下建一个 `teamspace-backup-<UTC时间>/` 目录，
    /// 内含数据库一致性快照、各团队工作空间下岗位文件夹的副本与 `manifest.json`。
    ///
    /// 数据库用 `VACUUM INTO` 生成快照：读的是一致性视图，不会复制到正在写的半个文件。
    /// 返回备份目录与摘要（团队与工作空间路径列表、文件数与大小）。
    pub fn export_backup(&self, input: ExportBackupInput) -> Result<BackupExportResult> {
        let dest = resolve_dir(&input.dest_dir, "备份目标目录")?;
        fs::create_dir_all(&dest).map_err(|err| {
            Error::internal(format!("无法创建备份目录 {}：{err}", dest.display()))
        })?;
        self.export_into(&dest, BACKUP_PREFIX, input.app_version)
    }

    /// 读取备份摘要（恢复前给 Human 看的那一份）：读清单并核对格式与 schema 版本。
    pub fn inspect_backup(&self, input: InspectBackupInput) -> Result<BackupSummary> {
        let dir = resolve_existing_dir(&input.backup_dir, "备份目录")?;
        let manifest = read_manifest(&dir)?;
        check_manifest(&manifest)?;
        Ok(summarize(&dir, &manifest))
    }

    /// 从备份恢复：先校验清单，再自动导出「恢复前备份」，然后恢复数据库与岗位文件夹。
    ///
    /// 备份的 schema 比当前程序新时拒绝恢复；比当前程序旧时，恢复后由重新打开数据库时的
    /// 迁移补齐（[`store::open`] 会迁移到当前版本）。
    pub fn restore_backup(&self, input: RestoreBackupInput) -> Result<BackupRestoreResult> {
        let dir = resolve_existing_dir(&input.backup_dir, "备份目录")?;
        let manifest = read_manifest(&dir)?;
        check_manifest(&manifest)?;

        // 恢复前先自动导出一份当前数据：恢复出问题时可以退回去。
        let pre = self.pre_restore_backup(&dir)?;

        let mut notes: Vec<String> = Vec::new();
        if manifest.schema_version < store::SCHEMA_VERSION {
            notes.push(format!(
                "备份来自较早的版本（schema {}），恢复后会自动迁移到当前版本（{}）。",
                manifest.schema_version,
                store::SCHEMA_VERSION
            ));
        }

        // 恢复数据库。
        let db_source = dir.join(&manifest.database_file);
        self.replace_database(&db_source)?;

        // 岗位文件夹写回原工作空间。
        let (restored, skipped) = self.restore_folders(&dir, &manifest);

        notes.push(format!("恢复前备份已保存到：{}", pre.backup_dir));
        if !restored.is_empty() {
            notes.push(format!("已写回 {} 个岗位文件夹。", restored.len()));
        }
        if !skipped.is_empty() {
            notes.push(format!(
                "有 {} 个岗位文件夹没能写回（见跳过清单）。",
                skipped.len()
            ));
        }

        Ok(BackupRestoreResult {
            backup_dir: dir.display().to_string(),
            schema_version: manifest.schema_version,
            pre_restore_backup_dir: pre.backup_dir,
            restored_folders: restored,
            skipped_folders: skipped,
            database_restored: true,
            // 数据库已换新，执行与消息的内存状态还是旧的：重启一次最稳妥。
            requires_restart: true,
            notes,
        })
    }

    /// 导出到已确定的目标目录（`export_backup` 与恢复前备份共用）。
    ///
    /// `prefix` 决定目录名前缀：正常导出是 `teamspace-backup-`，
    /// 恢复前自动备份是 `teamspace-pre-restore-`。
    fn export_into(
        &self,
        dest: &Path,
        prefix: &str,
        app_version: Option<String>,
    ) -> Result<BackupExportResult> {
        let stamp = timestamp();
        let backup_dir = unique_dir(dest, prefix, &stamp)?;
        fs::create_dir_all(&backup_dir).map_err(|err| {
            Error::internal(format!("无法创建备份目录 {}：{err}", backup_dir.display()))
        })?;

        // 1. 数据库一致性快照。
        let db_dir = backup_dir.join(DATABASE_DIR);
        fs::create_dir_all(&db_dir)
            .map_err(|err| Error::internal(format!("无法创建 {}：{err}", db_dir.display())))?;
        let db_file = db_dir.join(DATABASE_FILE);
        self.snapshot_database(&db_file)?;

        // 2. 岗位文件夹副本。
        let inventory = backup_svc::inventory(&self.lock())?;
        let mut files: Vec<BackupFile> = Vec::new();
        let mut teams: Vec<BackupTeam> = Vec::with_capacity(inventory.len());
        for team in inventory {
            let mut folders: Vec<BackupRoleFolder> = Vec::with_capacity(team.folders.len());
            for role in team.folders {
                let source = join_relative(Path::new(&team.workspace_path), &role.relative_path);
                let folder_name = role
                    .relative_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&role.relative_path)
                    .to_owned();
                let backup_path = format!("{WORKSPACES_DIR}/{}/{}", team.team_id, folder_name);
                let target = join_relative(&backup_dir, &backup_path);
                let mut entry = BackupRoleFolder {
                    role_id: role.role_id,
                    role_name: role.role_name,
                    relative_path: role.relative_path,
                    backup_path: backup_path.clone(),
                    file_count: 0,
                    total_bytes: 0,
                    problem: None,
                };
                if !source.is_dir() {
                    // 工作空间不可用或文件夹还没建：记下原因，不让整次导出失败。
                    entry.problem =
                        Some(format!("岗位文件夹不存在或不可访问：{}", source.display()));
                } else {
                    match copy_tree(&source, &target, Some(&backup_dir), &mut files) {
                        Ok((count, bytes)) => {
                            entry.file_count = count;
                            entry.total_bytes = bytes;
                        }
                        Err(err) => {
                            entry.problem = Some(err.to_string());
                        }
                    }
                }
                folders.push(entry);
            }
            teams.push(BackupTeam {
                team_id: team.team_id,
                team_name: team.team_name,
                workspace_path: team.workspace_path,
                role_folders: folders,
            });
        }

        // 数据库快照也进文件清单。
        let db_size = fs::metadata(&db_file).map(|meta| meta.len()).unwrap_or(0);
        files.push(BackupFile {
            path: format!("{DATABASE_DIR}/{DATABASE_FILE}"),
            size: db_size,
        });

        let exported_at = store::now();
        let manifest = BackupManifest {
            format_version: BACKUP_FORMAT_VERSION,
            app_version: app_version
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned()),
            schema_version: store::SCHEMA_VERSION,
            exported_at: exported_at.clone(),
            database_file: format!("{DATABASE_DIR}/{DATABASE_FILE}"),
            teams: teams.clone(),
            files: files.clone(),
        };
        let manifest_path = backup_dir.join(MANIFEST_FILE);
        let text = serde_json::to_string_pretty(&manifest)
            .map_err(|err| Error::internal(format!("生成备份清单失败：{err}")))?;
        fs::write(&manifest_path, text).map_err(|err| {
            Error::internal(format!("写入 {} 失败：{err}", manifest_path.display()))
        })?;

        let total_bytes = files.iter().map(|file| file.size).sum();
        Ok(BackupExportResult {
            backup_dir: backup_dir.display().to_string(),
            manifest_path: manifest_path.display().to_string(),
            database_path: db_file.display().to_string(),
            exported_at,
            schema_version: manifest.schema_version,
            team_count: teams.len() as u32,
            file_count: files.len() as u32,
            total_bytes,
            teams,
        })
    }

    /// 数据库一致性快照：`VACUUM INTO` 读的是一致性视图，不会拿到写了一半的文件。
    fn snapshot_database(&self, target: &Path) -> Result<()> {
        let conn = self.lock();
        // `VACUUM INTO` 的目标必须不存在；目标路径里的单引号要转义。
        let literal = target.display().to_string().replace('\'', "''");
        conn.execute_batch(&format!("VACUUM INTO '{literal}'"))
            .map_err(|err| {
                Error::internal(format!("生成数据库快照 {} 失败：{err}", target.display()))
            })?;
        Ok(())
    }

    /// 恢复前的自动备份：优先放在被恢复备份的旁边（方便一起拷走），
    /// 那个目录不可写时退回应用数据目录。
    fn pre_restore_backup(&self, backup_dir: &Path) -> Result<BackupExportResult> {
        let parent = backup_dir.parent().unwrap_or(Path::new("."));
        match self.export_into(parent, PRE_RESTORE_PREFIX, None) {
            Ok(pre) => Ok(pre),
            Err(err) => {
                let fallback = self.data_dir.join(PRE_RESTORE_FALLBACK_DIR);
                fs::create_dir_all(&fallback).map_err(|fallback_err| {
                    Error::internal(format!(
                        "无法创建 {}：{fallback_err}（原失败原因：{err}）",
                        fallback.display()
                    ))
                })?;
                self.export_into(&fallback, PRE_RESTORE_PREFIX, None)
            }
        }
    }

    /// 用备份里的数据库覆盖当前数据库。
    ///
    /// 顺序很重要：先把 WAL 检查点回主库文件，再关掉旧连接（否则旧连接往 WAL 写会污染
    /// 新文件），然后覆盖主库文件、清掉残留的 WAL，最后重新打开——重新打开时顺带跑迁移，
    /// 所以旧版本备份恢复后自动升级到当前 schema。
    ///
    /// 覆盖前先把原库另存一份：新库打不开时（文件损坏、schema 不认识等）把原库放回去，
    /// 应用不会停在「连着一个空库」的状态。
    fn replace_database(&self, source: &Path) -> Result<()> {
        if !source.is_file() {
            return Err(Error::not_found(format!(
                "备份里没有数据库快照：{}",
                source.display()
            )));
        }
        let db_path = self.db_path.clone();
        let mut guard = self.lock();
        // 已提交的数据先写回主库文件。
        guard
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|err| Error::internal(format!("整理数据库日志失败：{err}")))?;
        // 换掉旧连接：先放一个内存库占位，旧连接随即关闭（析构时会自己删掉 WAL）。
        let placeholder = Connection::open_in_memory()
            .map_err(|err| Error::internal(format!("无法打开临时数据库：{err}")))?;
        let old = std::mem::replace(&mut *guard, placeholder);
        drop(old);

        // 覆盖前先把原库另存一份：新库打不开时（文件损坏、schema 不认识等）把原库放回去，
        // 应用不会停在「连着一个空库」的状态。
        let original = db_path.with_extension("db.restore-orig");
        fs::copy(&db_path, &original).map_err(|err| {
            Error::internal(format!("备份原数据库到 {} 失败：{err}", original.display()))
        })?;

        // 覆盖主库文件：先写临时文件再改名，中途失败不会留下半个数据库。
        let temp = db_path.with_extension("db.restore-tmp");
        fs::copy(source, &temp).map_err(|err| {
            Error::internal(format!("复制备份数据库到 {} 失败：{err}", temp.display()))
        })?;
        // 旧库遗留的 WAL 与新库无关，清掉。
        for suffix in ["db-wal", "db-shm"] {
            let _ = fs::remove_file(db_path.with_extension(suffix));
        }
        fs::rename(&temp, &db_path).map_err(|err| {
            let _ = fs::remove_file(&temp);
            Error::internal(format!("替换数据库 {} 失败：{err}", db_path.display()))
        })?;

        // 重新打开：顺带迁移到当前 schema。
        // 新库打不开时把原库放回去，应用不会停在「连着一个空库」的状态。
        match store::open(&db_path) {
            Ok(fresh) => {
                *guard = fresh;
                let _ = fs::remove_file(&original);
                Ok(())
            }
            Err(err) => {
                // 退回原库：先把它放回原位再重新打开。
                let back = fs::rename(&original, &db_path)
                    .map_err(|restore_err| {
                        Error::internal(format!(
                            "恢复数据库失败（{err}），并且无法退回原数据库（{restore_err}）：{}",
                            db_path.display()
                        ))
                    })
                    .and_then(|_| store::open(&db_path));
                match back {
                    Ok(previous) => {
                        *guard = previous;
                        Err(Error::internal(format!(
                            "恢复数据库失败，已退回原数据库：{err}"
                        )))
                    }
                    Err(back_err) => Err(Error::internal(format!(
                        "恢复数据库失败（{err}），退回原数据库也失败：{back_err}"
                    ))),
                }
            }
        }
    }

    /// 岗位文件夹写回原工作空间；工作空间不存在时跳过并记入结果。
    fn restore_folders(
        &self,
        backup_dir: &Path,
        manifest: &BackupManifest,
    ) -> (Vec<String>, Vec<SkippedRoleFolder>) {
        let mut restored: Vec<String> = Vec::new();
        let mut skipped: Vec<SkippedRoleFolder> = Vec::new();
        for team in &manifest.teams {
            let workspace = PathBuf::from(&team.workspace_path);
            let workspace_missing = !workspace.is_dir();
            for folder in &team.role_folders {
                if workspace_missing {
                    skipped.push(SkippedRoleFolder {
                        role_id: folder.role_id.clone(),
                        role_name: folder.role_name.clone(),
                        workspace_path: team.workspace_path.clone(),
                        reason: format!("工作空间不存在：{}", team.workspace_path),
                    });
                    continue;
                }
                let source = join_relative(backup_dir, &folder.backup_path);
                if !source.is_dir() {
                    skipped.push(SkippedRoleFolder {
                        role_id: folder.role_id.clone(),
                        role_name: folder.role_name.clone(),
                        workspace_path: team.workspace_path.clone(),
                        reason: "备份里没有这个岗位文件夹".to_owned(),
                    });
                    continue;
                }
                let target = join_relative(&workspace, &folder.relative_path);
                let mut files = Vec::new();
                match copy_tree(&source, &target, None, &mut files) {
                    Ok(_) => restored.push(folder.relative_path.clone()),
                    Err(err) => skipped.push(SkippedRoleFolder {
                        role_id: folder.role_id.clone(),
                        role_name: folder.role_name.clone(),
                        workspace_path: team.workspace_path.clone(),
                        reason: err.to_string(),
                    }),
                }
            }
        }
        (restored, skipped)
    }
}

/// 清单校验：格式版本与 schema 版本都比当前程序新时拒绝。
fn check_manifest(manifest: &BackupManifest) -> Result<()> {
    if manifest.format_version > BACKUP_FORMAT_VERSION {
        return Err(Error::conflict(format!(
            "备份格式版本为 {}，当前应用只支持到 {}：请升级 Teamspace 后再恢复",
            manifest.format_version, BACKUP_FORMAT_VERSION
        )));
    }
    if manifest.schema_version > store::SCHEMA_VERSION {
        return Err(Error::conflict(format!(
            "备份来自更新版本的 Teamspace（数据库版本 {}，当前应用支持到 {}）：请先升级应用再恢复",
            manifest.schema_version,
            store::SCHEMA_VERSION
        )));
    }
    Ok(())
}

/// 清单 → 摘要。
fn summarize(dir: &Path, manifest: &BackupManifest) -> BackupSummary {
    BackupSummary {
        backup_dir: dir.display().to_string(),
        format_version: manifest.format_version,
        app_version: manifest.app_version.clone(),
        schema_version: manifest.schema_version,
        exported_at: manifest.exported_at.clone(),
        team_count: manifest.teams.len() as u32,
        file_count: manifest.files.len() as u32,
        total_bytes: manifest.files.iter().map(|file| file.size).sum(),
        teams: manifest.teams.clone(),
        needs_migration: manifest.schema_version < store::SCHEMA_VERSION,
        database_file: manifest.database_file.clone(),
    }
}

/// 读清单。
fn read_manifest(dir: &Path) -> Result<BackupManifest> {
    let path = dir.join(MANIFEST_FILE);
    let text = fs::read_to_string(&path).map_err(|err| {
        Error::not_found(format!(
            "备份目录里没有 {MANIFEST_FILE}（{}）：{err}",
            path.display()
        ))
    })?;
    serde_json::from_str(&text)
        .map_err(|err| Error::invalid(format!("备份清单不是合法的 JSON：{err}")))
}

/// 目录路径：展开 `~`、转成绝对路径；不存在时先不报错（导出时会创建）。
fn resolve_dir(path: &str, label: &str) -> Result<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(Error::invalid(format!("{label}不能为空")));
    }
    let expanded = expand_tilde(trimmed);
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(expanded)
    };
    Ok(normalize(&absolute))
}

/// 已存在的目录：不存在时按 not_found 报错。
fn resolve_existing_dir(path: &str, label: &str) -> Result<PathBuf> {
    let dir = resolve_dir(path, label)?;
    if !dir.is_dir() {
        return Err(Error::not_found(format!(
            "{label}不存在：{}",
            dir.display()
        )));
    }
    Ok(dir)
}

/// 展开 `~`（只展开到用户主目录）。
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix('~')
        && let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

/// 去掉路径里多余的 `.` 与 `..`（不碰符号链接）。
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

/// 相对路径（`/` 分隔）拼到目录上。
fn join_relative(base: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .fold(base.to_path_buf(), |path, part| path.join(part))
}

/// 不存在的目录名时加序号（`-2`、`-3`…），避免同一秒内两次导出互相覆盖。
fn unique_dir(parent: &Path, prefix: &str, stamp: &str) -> Result<PathBuf> {
    let base = parent.join(format!("{prefix}{stamp}"));
    if !base.exists() {
        return Ok(base);
    }
    for n in 2..1000 {
        let candidate = parent.join(format!("{prefix}{stamp}-{n}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(Error::conflict(format!(
        "{} 下已经有太多同名的备份目录了",
        parent.display()
    )))
}

/// 目录树复制：普通文件逐个复制，符号链接跳过（不跟随到工作空间外面去）。
///
/// 返回（文件数、总字节数）。导出时 `base` 给备份目录，收集备份内文件的相对路径与大小；
/// 恢复时 `base` 给 `None`——写回的目标在工作空间里，不在备份目录里。
fn copy_tree(
    src: &Path,
    dst: &Path,
    base: Option<&Path>,
    files: &mut Vec<BackupFile>,
) -> Result<(u32, u64)> {
    fs::create_dir_all(dst)
        .map_err(|err| Error::internal(format!("无法创建 {}：{err}", dst.display())))?;
    let mut entries: Vec<PathBuf> = fs::read_dir(src)
        .map_err(|err| Error::internal(format!("无法读取 {}：{err}", src.display())))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    let mut count = 0u32;
    let mut bytes = 0u64;
    for path in entries {
        let meta = fs::symlink_metadata(&path)
            .map_err(|err| Error::internal(format!("无法读取 {}：{err}", path.display())))?;
        let Some(name) = path.file_name() else {
            continue;
        };
        let target = dst.join(name);
        if meta.is_dir() {
            let (sub_count, sub_bytes) = copy_tree(&path, &target, base, files)?;
            count += sub_count;
            bytes += sub_bytes;
        } else if meta.is_file() {
            fs::copy(&path, &target).map_err(|err| {
                Error::internal(format!(
                    "复制 {} 到 {} 失败：{err}",
                    path.display(),
                    target.display()
                ))
            })?;
            if let Some(base) = base {
                files.push(BackupFile {
                    path: relative_to(base, &target)?,
                    size: meta.len(),
                });
            }
            count += 1;
            bytes += meta.len();
        }
    }
    Ok((count, bytes))
}

/// 相对备份目录的路径（`/` 分隔）。
fn relative_to(base: &Path, path: &Path) -> Result<String> {
    let relative = path.strip_prefix(base).map_err(|_| {
        Error::internal(format!(
            "{} 不在备份目录 {} 里",
            path.display(),
            base.display()
        ))
    })?;
    let mut parts = Vec::new();
    for part in relative.components() {
        parts.push(part.as_os_str().to_string_lossy().into_owned());
    }
    Ok(parts.join("/"))
}

/// 备份目录名用的时间戳（UTC，形如 `20261021-190218`）。
fn timestamp() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
}
