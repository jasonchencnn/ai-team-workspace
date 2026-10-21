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
//! 2. 校验备份内容：数据库快照要是一个能独立打开、完整性检查通过的 SQLite 文件，它的 schema 要与
//!    清单一致；清单里的文件清单要与备份目录里的文件（数量与大小）对得上；清单里的相对路径不能越出
//!    备份目录，团队工作空间要与备份数据库里的团队表一致（对不上就跳过并列出）；
//! 3. 有进行中的执行或投递中的消息时**拒绝**恢复（先取消或等它们结束）；
//! 4. 自动导出一份「恢复前备份」，恢复出问题时可以退回去；
//! 5. 恢复数据库（关掉旧连接 → 覆盖主库文件 → 重新打开并迁移）；
//! 6. 岗位文件夹按清单写回原工作空间；工作空间不存在时跳过，并在结果里列出。
//!
//! 备份与恢复都只由 Human 在应用里发起（Leader 无权，见 [`crate::dispatch`]）。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use crate::api::Teamspace;
use crate::error::{Error, Result};
use crate::model::{
    BackupExportResult, BackupFile, BackupManifest, BackupRestoreResult, BackupRoleFolder,
    BackupSummary, BackupTeam, BackupValidation, ExportBackupInput, InspectBackupInput,
    RestoreBackupInput, SkippedRoleFolder,
};
use crate::service::backup as backup_svc;
use crate::service::message as message_svc;
use crate::service::run as run_svc;
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

    /// 读取备份摘要（恢复前给 Human 看的那一份）：读清单并核对格式与 schema 版本，
    /// 顺带把备份内容的校验结果（快照完整性、文件清单、路径安全）一并给出。
    pub fn inspect_backup(&self, input: InspectBackupInput) -> Result<BackupSummary> {
        let dir = resolve_existing_dir(&input.backup_dir, "备份目录")?;
        let manifest = read_manifest(&dir)?;
        check_manifest(&manifest)?;
        Ok(summarize(&dir, &manifest))
    }

    /// 从备份恢复：先校验清单与备份内容，再自动导出「恢复前备份」，然后恢复数据库与岗位文件夹。
    ///
    /// 备份的 schema 比当前程序新时拒绝恢复；比当前程序旧时，恢复后由重新打开数据库时的
    /// 迁移补齐（[`store::open`] 会迁移到当前版本）。
    ///
    /// 有进行中的执行或投递中的消息时拒绝恢复——覆盖数据库会把正在跑的活儿踩掉。
    pub fn restore_backup(&self, input: RestoreBackupInput) -> Result<BackupRestoreResult> {
        let dir = resolve_existing_dir(&input.backup_dir, "备份目录")?;
        let manifest = read_manifest(&dir)?;
        check_manifest(&manifest)?;

        // 有进行中的执行或投递中的消息时拒绝：覆盖数据库会踩掉正在跑的活儿。
        self.ensure_idle_for_restore()?;

        // 恢复前校验（在做恢复前备份之前）：内容不过关就不动任何数据。
        let checked = check_backup_content(&dir, &manifest);
        if !checked.blocking.is_empty() {
            return Err(Error::invalid(format!(
                "备份没有通过校验，不能恢复：{}",
                checked.blocking.join("；")
            )));
        }

        let mut notes: Vec<String> = Vec::new();
        if manifest.schema_version < store::SCHEMA_VERSION {
            notes.push(format!(
                "备份来自较早的版本（schema {}），恢复后会自动迁移到当前版本（{}）。",
                manifest.schema_version,
                store::SCHEMA_VERSION
            ));
        }

        // 恢复前先自动导出一份当前数据：恢复出问题时可以退回去。
        let pre = self.pre_restore_backup(&dir)?;

        // 恢复数据库。
        self.replace_database(&checked.database_path)?;

        // 岗位文件夹写回原工作空间。
        let (restored, skipped) = self.restore_folders(&dir, &checked);

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

    /// 恢复前确认没有进行中的执行，也没有投递中的消息。
    ///
    /// 覆盖数据库会让正在跑的执行写进被换掉的库，也会把投递中的消息吞掉：宁可让 Human
    /// 先取消或等它们结束。
    fn ensure_idle_for_restore(&self) -> Result<()> {
        let conn = self.lock();
        let active = run_svc::all_active(&conn)?;
        let pending = message_svc::pending_count(&conn)?;
        if active.is_empty() && pending == 0 {
            return Ok(());
        }
        let mut reasons = Vec::new();
        if !active.is_empty() {
            reasons.push(format!("还有 {} 个执行正在进行", active.len()));
        }
        if pending > 0 {
            reasons.push(format!("还有 {pending} 条消息正在投递"));
        }
        Err(Error::conflict(format!(
            "{}：请先取消或等它们结束，再恢复备份",
            reasons.join("，")
        )))
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
        // 备份内的岗位文件夹路径不能重名：重名会让清单与目录对不上（恢复时会被校验挡住）。
        let mut used_paths: Vec<String> = Vec::new();
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
                let backup_path = unique_backup_path(
                    &format!("{WORKSPACES_DIR}/{}/{}", team.team_id, folder_name),
                    &mut used_paths,
                );
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
    /// 覆盖前先把原库另存一份：任何一步失败都把原库放回去并**重新打开原库连接**，应用不会停在
    /// 「连着一个空库」的状态——空库看起来像数据全没了，比恢复失败更吓人。
    fn replace_database(&self, source: &Path) -> Result<()> {
        if !source.is_file() {
            return Err(Error::not_found(format!(
                "备份里没有数据库快照：{}",
                source.display()
            )));
        }
        let db_path = self.db_path.clone();
        let original = db_path.with_extension("db.restore-orig");
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

        // 从这里开始的每一条失败路径都要把原库放回并重新打开原库连接。
        // `saved` 表示原库有没有完整地另存过：只有完整另存过才能拿它去覆盖原库。
        let (failure, saved) = match self.overwrite_database(&db_path, &original, source) {
            Ok(()) => match store::open(&db_path) {
                Ok(fresh) => {
                    *guard = fresh;
                    let _ = fs::remove_file(&original);
                    return Ok(());
                }
                Err(err) => (format!("恢复数据库失败（{err}）"), true),
            },
            Err((err, saved)) => (err.to_string(), saved),
        };

        // 失败：把原库放回原位并重新打开它，应用继续连着原来的数据。
        match restore_original_database(&db_path, &original, saved) {
            Ok(previous) => {
                *guard = previous;
                Err(Error::internal(format!("{failure}，已退回原数据库")))
            }
            Err(back_err) => Err(Error::internal(format!(
                "{failure}，退回原数据库也失败：{back_err}"
            ))),
        }
    }

    /// 覆盖主库文件：先另存原库，再写临时文件，最后换名。
    ///
    /// 换名是原子的：中途失败不会留下半个数据库。出错时同时给出「原库有没有完整另存」——
    /// 只有完整另存过才能拿它去覆盖原库（另存复制到一半失败时留下的是半个文件）。
    fn overwrite_database(
        &self,
        db_path: &Path,
        original: &Path,
        source: &Path,
    ) -> std::result::Result<(), (Error, bool)> {
        // 覆盖前先把原库另存一份：新库打不开时（文件损坏、schema 不认识等）把原库放回去，
        // 应用不会停在「连着一个空库」的状态。
        #[cfg(test)]
        if take_restore_fail_point(RestoreFailPoint::SaveOriginal) {
            // 模拟「复制到一半失败」：原库的位置上会留下半个文件。
            let _ = fs::write(original, b"partial");
            return Err((Error::internal("另存原数据库失败"), false));
        }
        if let Err(err) = fs::copy(db_path, original) {
            // 复制失败可能留下半个文件：删掉它，回退时不能拿半个库去覆盖原库。
            let _ = fs::remove_file(original);
            return Err((
                Error::internal(format!("备份原数据库到 {} 失败：{err}", original.display())),
                false,
            ));
        }

        // 覆盖主库文件：先写临时文件再改名，中途失败不会留下半个数据库。
        let temp = db_path.with_extension("db.restore-tmp");
        #[cfg(test)]
        if take_restore_fail_point(RestoreFailPoint::CopySnapshot) {
            return Err((Error::internal("复制备份数据库失败"), true));
        }
        if let Err(err) = fs::copy(source, &temp) {
            let _ = fs::remove_file(&temp);
            return Err((
                Error::internal(format!("复制备份数据库到 {} 失败：{err}", temp.display())),
                true,
            ));
        }
        // 旧库遗留的 WAL 与新库无关，清掉。
        for suffix in ["db-wal", "db-shm"] {
            let _ = fs::remove_file(db_path.with_extension(suffix));
        }
        #[cfg(test)]
        if take_restore_fail_point(RestoreFailPoint::ReplaceMain) {
            let _ = fs::remove_file(&temp);
            return Err((Error::internal("替换数据库文件失败"), true));
        }
        if let Err(err) = fs::rename(&temp, db_path) {
            let _ = fs::remove_file(&temp);
            return Err((
                Error::internal(format!("替换数据库 {} 失败：{err}", db_path.display())),
                true,
            ));
        }
        Ok(())
    }

    /// 岗位文件夹写回原工作空间；工作空间不存在时跳过并记入结果。
    fn restore_folders(
        &self,
        backup_dir: &Path,
        checked: &CheckedBackup,
    ) -> (Vec<String>, Vec<SkippedRoleFolder>) {
        let mut restored: Vec<String> = Vec::new();
        // 校验阶段就已经判定不能写的岗位文件夹，原样带进结果。
        let mut skipped: Vec<SkippedRoleFolder> = checked.skipped.clone();
        for team in &checked.teams {
            let workspace = PathBuf::from(&team.workspace_path);
            if !workspace.is_dir() {
                for folder in &team.folders {
                    skipped.push(SkippedRoleFolder {
                        role_id: folder.role_id.clone(),
                        role_name: folder.role_name.clone(),
                        workspace_path: team.workspace_path.clone(),
                        reason: format!("工作空间不存在：{}", team.workspace_path),
                    });
                }
                continue;
            }
            // 基准目录取真实路径：工作空间里的符号链接不该把写入带到别处去。
            let base = workspace
                .canonicalize()
                .unwrap_or_else(|_| workspace.clone());
            for folder in &team.folders {
                let source =
                    match join_within(backup_dir, &folder.backup_path, "备份里的岗位文件夹路径")
                    {
                        Ok(path) => path,
                        Err(err) => {
                            skipped.push(skip(folder, &team.workspace_path, err.to_string()));
                            continue;
                        }
                    };
                if !source.is_dir() {
                    skipped.push(skip(
                        folder,
                        &team.workspace_path,
                        "备份里没有这个岗位文件夹".to_owned(),
                    ));
                    continue;
                }
                let target = match join_within(&base, &folder.relative_path, "岗位文件夹路径")
                {
                    Ok(path) => path,
                    Err(err) => {
                        skipped.push(skip(folder, &team.workspace_path, err.to_string()));
                        continue;
                    }
                };
                let mut files = Vec::new();
                match copy_tree(&source, &target, None, &mut files) {
                    Ok(_) => restored.push(folder.relative_path.clone()),
                    Err(err) => skipped.push(skip(folder, &team.workspace_path, err.to_string())),
                }
            }
        }
        (restored, skipped)
    }
}

/// 跳过的岗位文件夹（内部辅助）。
fn skip(folder: &CheckedFolder, workspace_path: &str, reason: String) -> SkippedRoleFolder {
    SkippedRoleFolder {
        role_id: folder.role_id.clone(),
        role_name: folder.role_name.clone(),
        workspace_path: workspace_path.to_owned(),
        reason,
    }
}

/// 清单校验后确认安全的岗位文件夹。
#[derive(Debug, Clone)]
struct CheckedFolder {
    role_id: String,
    role_name: String,
    /// 相对工作空间的路径（`Teamspace/<岗位文件夹>`）。
    relative_path: String,
    /// 相对备份目录的路径（`workspaces/<团队标识>/<岗位文件夹>`）。
    backup_path: String,
}

/// 清单校验后确认安全的团队。
#[derive(Debug, Clone)]
struct CheckedTeam {
    /// 工作空间绝对路径（已与备份数据库里的团队表核对过）。
    workspace_path: String,
    folders: Vec<CheckedFolder>,
}

/// 备份内容校验的结果。
struct CheckedBackup {
    /// 给 Human 看的校验结果。
    validation: BackupValidation,
    /// 致命问题：不解决就不能恢复。
    blocking: Vec<String>,
    /// 通过校验的团队与岗位文件夹。
    teams: Vec<CheckedTeam>,
    /// 没能通过校验的岗位文件夹（恢复时跳过并列出）。
    skipped: Vec<SkippedRoleFolder>,
    /// 数据库快照的绝对路径。
    database_path: PathBuf,
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

/// 备份内容的校验：数据库快照、文件清单、路径安全与工作空间核对。
///
/// 致命问题（快照不可用、文件清单对不上、数据库快照路径不安全）会让恢复被拒绝；
/// 岗位文件夹与工作空间的问题只跳过那一个文件夹，并在结果里逐条列出。
fn check_backup_content(dir: &Path, manifest: &BackupManifest) -> CheckedBackup {
    let mut blocking: Vec<String> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let mut skipped: Vec<SkippedRoleFolder> = Vec::new();

    // 1. 数据库快照：路径要安全，还要是一个能独立打开、完整性检查通过的 SQLite 文件。
    let database_path = match join_within(dir, &manifest.database_file, "数据库快照路径") {
        Ok(path) => path,
        Err(err) => {
            blocking.push(err.to_string());
            problems.push(err.to_string());
            PathBuf::new()
        }
    };
    let snapshot = check_database_snapshot(&database_path, manifest.schema_version);
    problems.extend(snapshot.problems.iter().cloned());
    let snapshot_ok = snapshot.readable
        && snapshot.schema_matches
        && snapshot.schema_version <= store::SCHEMA_VERSION;
    if !snapshot_ok {
        blocking.extend(snapshot.problems.iter().cloned());
    }

    // 2. 清单里的文件清单与备份目录要对得上（数量与大小）。
    let files_match = check_file_listing(dir, manifest, &mut problems);
    if !files_match {
        blocking.push("清单里的文件清单与备份目录里的文件对不上".to_owned());
    }

    // 3. 清单里的路径与工作空间。
    let teams = check_team_paths(dir, manifest, &mut problems, &mut skipped);

    let validation = BackupValidation {
        database_readable: snapshot.readable,
        database_integrity: snapshot.integrity,
        database_schema_version: snapshot.schema_version,
        schema_matches: snapshot.schema_matches,
        files_match,
        skipped_folders: skipped.len() as u32,
        restorable: blocking.is_empty(),
        problems,
    };
    CheckedBackup {
        validation,
        blocking,
        teams,
        skipped,
        database_path,
    }
}

/// 数据库快照的校验结果。
struct SnapshotCheck {
    /// 能不能独立打开并通过完整性检查。
    readable: bool,
    /// 完整性检查的结果（`ok` 或失败原因）。
    integrity: String,
    /// 快照的 schema 版本。
    schema_version: i64,
    /// 快照的 schema 版本与清单是否一致。
    schema_matches: bool,
    /// 发现的问题。
    problems: Vec<String>,
}

/// 校验备份里的数据库快照：能只读打开、完整性检查通过、schema 与清单一致且不比当前程序新。
fn check_database_snapshot(path: &Path, manifest_schema: i64) -> SnapshotCheck {
    let mut problems: Vec<String> = Vec::new();
    if path.as_os_str().is_empty() {
        return SnapshotCheck {
            readable: false,
            integrity: "路径不合法".to_owned(),
            schema_version: -1,
            schema_matches: false,
            problems: vec!["数据库快照的路径不合法".to_owned()],
        };
    }
    if !path.is_file() {
        let message = format!("备份里没有数据库快照：{}", path.display());
        return SnapshotCheck {
            readable: false,
            integrity: message.clone(),
            schema_version: -1,
            schema_matches: false,
            problems: vec![message],
        };
    }
    let conn = match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => conn,
        Err(err) => {
            let message = format!("数据库快照打不开：{err}");
            return SnapshotCheck {
                readable: false,
                integrity: message.clone(),
                schema_version: -1,
                schema_matches: false,
                problems: vec![message],
            };
        }
    };
    // 完整性检查：这个文件本身得是好的，不能是拷到一半的半个库。
    let integrity = match integrity_check(&conn) {
        Ok(text) => text,
        Err(err) => {
            let message = format!("数据库快照的完整性检查失败：{err}");
            return SnapshotCheck {
                readable: false,
                integrity: message.clone(),
                schema_version: -1,
                schema_matches: false,
                problems: vec![message],
            };
        }
    };
    let readable = integrity == "ok";
    if !readable {
        problems.push(format!("数据库快照没有通过完整性检查：{integrity}"));
    }
    let schema_version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap_or(-1);
    let schema_matches = schema_version == manifest_schema;
    if !schema_matches {
        problems.push(format!(
            "数据库快照的版本（{schema_version}）与清单里记的版本（{manifest_schema}）对不上"
        ));
    }
    if schema_version > store::SCHEMA_VERSION {
        problems.push(format!(
            "数据库快照来自更新版本的 Teamspace（版本 {schema_version}，当前应用支持到 {}）",
            store::SCHEMA_VERSION
        ));
    }
    SnapshotCheck {
        readable,
        integrity,
        schema_version,
        schema_matches,
        problems,
    }
}

/// `PRAGMA integrity_check`：返回 `ok` 表示这个库文件本身没坏。
fn integrity_check(conn: &Connection) -> Result<String> {
    let mut stmt = conn.prepare("PRAGMA integrity_check")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut messages: Vec<String> = Vec::new();
    for row in rows {
        messages.push(row?);
    }
    Ok(match messages.len() {
        0 => "没有结果".to_owned(),
        1 => messages.remove(0),
        _ => messages.join("；"),
    })
}

/// 清单里的文件清单与备份目录里的文件是否一致（数量与大小都对得上）。
fn check_file_listing(dir: &Path, manifest: &BackupManifest, problems: &mut Vec<String>) -> bool {
    let mut listed: BTreeMap<String, u64> = BTreeMap::new();
    let mut safe = true;
    for file in &manifest.files {
        if let Err(err) = safe_relative_parts(&file.path, "清单里的文件路径") {
            problems.push(err.to_string());
            safe = false;
            continue;
        }
        if listed.insert(file.path.clone(), file.size).is_some() {
            problems.push(format!("清单里有重复的文件条目：{}", file.path));
            safe = false;
        }
    }
    let mut actual: BTreeMap<String, u64> = BTreeMap::new();
    match scan_backup_files(dir, dir) {
        Ok(files) => {
            for file in files {
                actual.insert(file.path, file.size);
            }
        }
        Err(err) => {
            problems.push(format!("读取备份目录失败：{err}"));
            return false;
        }
    }
    if !safe {
        return false;
    }
    let mut ok = true;
    for (path, size) in &listed {
        match actual.get(path) {
            Some(actual_size) if actual_size == size => {}
            Some(actual_size) => {
                problems.push(format!(
                    "清单里 {path} 的大小是 {size} 字节，备份目录里是 {actual_size} 字节"
                ));
                ok = false;
            }
            None => {
                problems.push(format!("备份目录里没有清单中的文件：{path}"));
                ok = false;
            }
        }
    }
    for path in actual.keys() {
        if !listed.contains_key(path) {
            problems.push(format!("备份目录里有清单没写的文件：{path}"));
            ok = false;
        }
    }
    if listed.len() != actual.len() {
        problems.push(format!(
            "清单里写了 {} 个文件，备份目录里是 {} 个",
            listed.len(),
            actual.len()
        ));
        ok = false;
    }
    ok
}

/// 清单里的团队与岗位文件夹：相对路径要安全，工作空间要与备份数据库里的团队表一致。
fn check_team_paths(
    dir: &Path,
    manifest: &BackupManifest,
    problems: &mut Vec<String>,
    skipped: &mut Vec<SkippedRoleFolder>,
) -> Vec<CheckedTeam> {
    // 工作空间以备份内数据库的团队表为准，与清单交叉核对。
    let db_workspaces = backup_db_workspaces(dir, manifest);
    let mut teams: Vec<CheckedTeam> = Vec::new();
    for team in &manifest.teams {
        // 团队必须在备份数据库里，且工作空间路径一致。
        let team_reason: Option<String> = match &db_workspaces {
            None => Some("备份数据库打不开，无法核对团队的工作空间".to_owned()),
            Some(map) => match map.get(&team.team_id) {
                Some(path) if path == team.workspace_path.trim() => None,
                Some(path) => Some(format!(
                    "清单里的工作空间路径（{}）与备份数据库里的（{}）不一致",
                    team.workspace_path, path
                )),
                None => Some("备份数据库里没有这个团队".to_owned()),
            },
        };
        if let Some(reason) = team_reason {
            problems.push(format!("{}：{}", team.team_name, reason));
            for folder in &team.role_folders {
                skipped.push(SkippedRoleFolder {
                    role_id: folder.role_id.clone(),
                    role_name: folder.role_name.clone(),
                    workspace_path: team.workspace_path.clone(),
                    reason: reason.clone(),
                });
            }
            continue;
        }
        // 工作空间必须是绝对路径。
        if let Err(err) = check_workspace_path(&team.workspace_path) {
            problems.push(format!("{}：{}", team.team_name, err));
            for folder in &team.role_folders {
                skipped.push(SkippedRoleFolder {
                    role_id: folder.role_id.clone(),
                    role_name: folder.role_name.clone(),
                    workspace_path: team.workspace_path.clone(),
                    reason: err.to_string(),
                });
            }
            continue;
        }
        let mut folders: Vec<CheckedFolder> = Vec::new();
        for folder in &team.role_folders {
            // 清单里的相对路径必须安全：不许有 `..`、绝对路径或盘符。
            let reason = safe_relative_parts(&folder.relative_path, "岗位文件夹路径")
                .and_then(|_| safe_relative_parts(&folder.backup_path, "备份里的岗位文件夹路径"))
                .err();
            if let Some(reason) = reason {
                let reason = reason.to_string();
                problems.push(format!("{}：{}", folder.role_name, reason));
                skipped.push(SkippedRoleFolder {
                    role_id: folder.role_id.clone(),
                    role_name: folder.role_name.clone(),
                    workspace_path: team.workspace_path.clone(),
                    reason: reason.clone(),
                });
                continue;
            }
            folders.push(CheckedFolder {
                role_id: folder.role_id.clone(),
                role_name: folder.role_name.clone(),
                relative_path: folder.relative_path.clone(),
                backup_path: folder.backup_path.clone(),
            });
        }
        teams.push(CheckedTeam {
            workspace_path: team.workspace_path.clone(),
            folders,
        });
    }
    teams
}

/// 备份数据库里的团队工作空间（团队 id → 工作空间路径）。
fn backup_db_workspaces(dir: &Path, manifest: &BackupManifest) -> Option<BTreeMap<String, String>> {
    let path = join_within(dir, &manifest.database_file, "数据库快照路径").ok()?;
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let mut stmt = conn.prepare("SELECT id, workspace_path FROM teams").ok()?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .ok()?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (id, path) = row.ok()?;
        map.insert(id, path);
    }
    Some(map)
}

/// 清单 → 摘要（顺带做一遍内容校验，把校验结果一并给 Human）。
fn summarize(dir: &Path, manifest: &BackupManifest) -> BackupSummary {
    let checked = check_backup_content(dir, manifest);
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
        validation: checked.validation,
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

// ---------------------------------------------------------------- 路径安全

/// 清单里的相对路径：只允许普通组成部分。
///
/// 拒绝 `..`、绝对路径、Windows 盘符与空字节；`/` 与 `\` 都当分隔符。清单是外部文件，
/// 里面的路径一律当作不可信输入处理。
fn safe_relative_parts(relative: &str, label: &str) -> Result<Vec<String>> {
    let trimmed = relative.trim();
    if trimmed.is_empty() {
        return Err(Error::invalid(format!("{label}不能为空")));
    }
    if trimmed.contains('\0') {
        return Err(Error::invalid(format!("{label}里有空字节：{relative}")));
    }
    if trimmed.starts_with('/') || trimmed.starts_with('\\') {
        return Err(Error::invalid(format!(
            "{label}必须是相对路径，不能是绝对路径：{relative}"
        )));
    }
    let mut parts: Vec<String> = Vec::new();
    for part in trimmed.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(Error::invalid(format!("{label}不能包含「..」：{relative}")));
        }
        if is_drive_letter(part) {
            return Err(Error::invalid(format!("{label}不能包含盘符：{relative}")));
        }
        parts.push(part.to_owned());
    }
    if parts.is_empty() {
        return Err(Error::invalid(format!(
            "{label}不是有效的相对路径：{relative}"
        )));
    }
    Ok(parts)
}

/// Windows 盘符（`C:`、`C:foo`）。
fn is_drive_letter(part: &str) -> bool {
    let bytes = part.as_bytes();
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

/// 把清单里的相对路径拼到基准目录上，并确认结果仍在基准目录之内。
///
/// 先按白名单拆出组成部分，再拼到基准目录上，最后规范化比较——两道关都过了才算安全。
fn join_within(base: &Path, relative: &str, label: &str) -> Result<PathBuf> {
    let parts = safe_relative_parts(relative, label)?;
    let mut path = base.to_path_buf();
    for part in parts {
        path.push(part);
    }
    let normalized_base = normalize(base);
    let normalized = normalize(&path);
    if !normalized.starts_with(&normalized_base) {
        return Err(Error::invalid(format!(
            "{label}（{relative}）指向基准目录之外，已拒绝"
        )));
    }
    Ok(normalized)
}

/// 工作空间路径必须是绝对路径。
fn check_workspace_path(path: &str) -> Result<()> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(Error::invalid("工作空间路径不能为空"));
    }
    if trimmed.contains('\0') {
        return Err(Error::invalid("工作空间路径里有空字节"));
    }
    if !Path::new(trimmed).is_absolute() {
        return Err(Error::invalid(format!(
            "工作空间路径必须是绝对路径：{trimmed}"
        )));
    }
    Ok(())
}

/// 扫描备份目录里的文件（相对路径 + 大小）。
///
/// 跳过清单文件本身与符号链接：符号链接可能指向备份目录之外，导出时也不会复制它们。
fn scan_backup_files(root: &Path, current: &Path) -> Result<Vec<BackupFile>> {
    let mut out: Vec<BackupFile> = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(current)
        .map_err(|err| Error::internal(format!("无法读取 {}：{err}", current.display())))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    for path in entries {
        let meta = fs::symlink_metadata(&path)
            .map_err(|err| Error::internal(format!("无法读取 {}：{err}", path.display())))?;
        let Some(name) = path.file_name() else {
            continue;
        };
        if meta.is_dir() {
            out.extend(scan_backup_files(root, &path)?);
        } else if meta.is_file() {
            if current == root && name == MANIFEST_FILE {
                continue;
            }
            out.push(BackupFile {
                path: relative_to(root, &path)?,
                size: meta.len(),
            });
        }
    }
    Ok(out)
}

/// 备份内的岗位文件夹路径不能重名：重名会让清单与目录对不上（恢复时会被校验挡住）。
fn unique_backup_path(base: &str, used: &mut Vec<String>) -> String {
    if !used.iter().any(|path| path == base) {
        used.push(base.to_owned());
        return base.to_owned();
    }
    for n in 2..1000 {
        let candidate = format!("{base}-{n}");
        if !used.iter().any(|path| path == &candidate) {
            used.push(candidate.clone());
            return candidate;
        }
    }
    used.push(base.to_owned());
    base.to_owned()
}

/// 放回原库：清掉新库留下的 WAL，把另存的原库换回主库位置，再重新打开。
///
/// `saved` 为假表示原库没有完整另存（例如另存时复制到一半失败）：这时主库文件本身没被动过，
/// 直接重新打开它即可——拿半个文件去覆盖原库只会把好数据弄坏。
fn restore_original_database(db_path: &Path, original: &Path, saved: bool) -> Result<Connection> {
    // 新库可能建了 WAL 与共享内存文件，留着会污染放回来的原库。
    for suffix in ["db-wal", "db-shm"] {
        let _ = fs::remove_file(db_path.with_extension(suffix));
    }
    if saved && original.is_file() {
        fs::rename(original, db_path).map_err(|err| {
            Error::internal(format!("无法退回原数据库 {}：{err}", db_path.display()))
        })?;
    } else {
        // 没完整另存：主库文件没被动过，把那个半个文件清掉。
        let _ = fs::remove_file(original);
    }
    store::open(db_path)
}

// ---------------------------------------------------------------- 测试钩子

/// 覆盖数据库时的失败注入点（仅测试用）。
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestoreFailPoint {
    /// 另存原库时失败。
    SaveOriginal,
    /// 复制备份快照到临时文件时失败。
    CopySnapshot,
    /// 临时文件换名覆盖主库时失败。
    ReplaceMain,
}

#[cfg(test)]
thread_local! {
    /// 覆盖过程中的失败注入点（每个测试线程各一份，互不干扰）。
    static RESTORE_FAIL_POINT: std::cell::Cell<Option<RestoreFailPoint>> =
        const { std::cell::Cell::new(None) };
}

/// 仅测试用：让下一次覆盖数据库在指定步骤失败。
#[cfg(test)]
pub(crate) fn fail_restore_at(point: RestoreFailPoint) {
    RESTORE_FAIL_POINT.with(|slot| slot.set(Some(point)));
}

/// 仅测试用：清掉失败注入点。
#[cfg(test)]
pub(crate) fn clear_restore_fail_point() {
    RESTORE_FAIL_POINT.with(|slot| slot.set(None));
}

/// 当前线程是否要在这一步失败（命中后清掉注入点，只失败这一次）。
#[cfg(test)]
fn take_restore_fail_point(point: RestoreFailPoint) -> bool {
    RESTORE_FAIL_POINT.with(|slot| {
        if slot.get() == Some(point) {
            slot.set(None);
            true
        } else {
            false
        }
    })
}
