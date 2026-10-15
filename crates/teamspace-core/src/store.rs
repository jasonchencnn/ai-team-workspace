//! SQLite 存储：连接配置、schema 与版本迁移、通用工具。
//!
//! schema 版本记录在 `PRAGMA user_version`；每个版本一段 SQL，按顺序在事务中执行。

use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;
use serde::de::DeserializeOwned;
use time::OffsetDateTime;

use crate::error::{Error, Result};

/// 当前 schema 版本。
pub(crate) const SCHEMA_VERSION: i64 = 9;

const SCHEMA_V1: &str = r#"
CREATE TABLE teams (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL CHECK (name <> ''),
    description    TEXT NOT NULL DEFAULT '',
    workspace_path TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    archived_at    TEXT
);
CREATE UNIQUE INDEX teams_active_name ON teams (name) WHERE archived_at IS NULL;

CREATE TABLE harnesses (
    id                TEXT PRIMARY KEY,
    team_id           TEXT NOT NULL REFERENCES teams (id),
    name              TEXT NOT NULL CHECK (name <> ''),
    kind              TEXT NOT NULL CHECK (kind IN ('claude_code', 'codex', 'mock')),
    executable        TEXT,
    model             TEXT,
    profile           TEXT,
    health_status     TEXT NOT NULL DEFAULT 'unknown' CHECK (health_status IN ('ok', 'error', 'unknown')),
    health_version    TEXT,
    health_detail     TEXT,
    health_checked_at TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    archived_at       TEXT
);
CREATE UNIQUE INDEX harnesses_active_name ON harnesses (team_id, name) WHERE archived_at IS NULL;

CREATE TABLE roles (
    id          TEXT PRIMARY KEY,
    team_id     TEXT NOT NULL REFERENCES teams (id),
    name        TEXT NOT NULL CHECK (name <> ''),
    code        TEXT NOT NULL,
    task_prefix TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    guidelines  TEXT NOT NULL DEFAULT '',
    is_leader   INTEGER NOT NULL DEFAULT 0 CHECK (is_leader IN (0, 1)),
    harness_id  TEXT REFERENCES harnesses (id),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    archived_at TEXT
);
CREATE UNIQUE INDEX roles_code ON roles (team_id, code);
CREATE UNIQUE INDEX roles_active_prefix ON roles (team_id, task_prefix) WHERE archived_at IS NULL;
CREATE UNIQUE INDEX roles_active_leader ON roles (team_id) WHERE is_leader = 1 AND archived_at IS NULL;
CREATE INDEX roles_harness ON roles (harness_id);

CREATE TABLE task_sequences (
    team_id TEXT NOT NULL REFERENCES teams (id),
    prefix  TEXT NOT NULL,
    last    INTEGER NOT NULL,
    PRIMARY KEY (team_id, prefix)
);

CREATE TABLE tasks (
    id             TEXT PRIMARY KEY,
    team_id        TEXT NOT NULL REFERENCES teams (id),
    key            TEXT NOT NULL,
    title          TEXT NOT NULL CHECK (title <> ''),
    description    TEXT NOT NULL DEFAULT '',
    status         TEXT NOT NULL CHECK (status IN ('todo', 'ready', 'doing', 'blocked', 'review', 'done', 'cancelled')),
    owner_role_id  TEXT NOT NULL REFERENCES roles (id),
    blocked_reason TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    UNIQUE (team_id, key),
    CHECK ((status = 'blocked') = (blocked_reason IS NOT NULL))
);
CREATE INDEX tasks_team_status ON tasks (team_id, status);
CREATE INDEX tasks_owner ON tasks (owner_role_id);

CREATE TABLE task_file_refs (
    task_id  TEXT NOT NULL REFERENCES tasks (id),
    position INTEGER NOT NULL,
    path     TEXT NOT NULL CHECK (path <> ''),
    anchor   TEXT,
    PRIMARY KEY (task_id, position)
);

CREATE TABLE task_dependencies (
    task_id       TEXT NOT NULL REFERENCES tasks (id),
    depends_on_id TEXT NOT NULL REFERENCES tasks (id),
    position      INTEGER NOT NULL,
    PRIMARY KEY (task_id, depends_on_id),
    CHECK (task_id <> depends_on_id)
);
CREATE INDEX task_dependencies_upstream ON task_dependencies (depends_on_id);

CREATE TABLE events (
    id         TEXT PRIMARY KEY,
    team_id    TEXT NOT NULL REFERENCES teams (id),
    task_id    TEXT REFERENCES tasks (id),
    role_id    TEXT REFERENCES roles (id),
    kind       TEXT NOT NULL,
    actor      TEXT NOT NULL,
    summary    TEXT NOT NULL,
    data       TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX events_team ON events (team_id, id);
CREATE INDEX events_task ON events (task_id, id);
"#;

/// v2：活动记录增加自增序号 `seq` 作为排序与分页游标键（UUID v7 只保证毫秒级有序）。
const SCHEMA_V2: &str = r#"
CREATE TABLE events_v2 (
    seq        INTEGER PRIMARY KEY,
    id         TEXT NOT NULL UNIQUE,
    team_id    TEXT NOT NULL REFERENCES teams (id),
    task_id    TEXT REFERENCES tasks (id),
    role_id    TEXT REFERENCES roles (id),
    kind       TEXT NOT NULL,
    actor      TEXT NOT NULL,
    summary    TEXT NOT NULL,
    data       TEXT NOT NULL,
    created_at TEXT NOT NULL
);
INSERT INTO events_v2 (id, team_id, task_id, role_id, kind, actor, summary, data, created_at)
    SELECT id, team_id, task_id, role_id, kind, actor, summary, data, created_at
    FROM events ORDER BY id;
DROP TABLE events;
ALTER TABLE events_v2 RENAME TO events;
CREATE INDEX events_team ON events (team_id, seq);
CREATE INDEX events_task ON events (task_id, seq);
"#;

/// v3：岗位文件夹（D14）。`folder` 为相对工作空间的文件夹路径（未建立时为 NULL），
/// `folder_problem` 为最近一次同步失败的说明，`journaled_seq` 为已写入工作日志的最后一条活动序号。
/// 已有岗位迁移后 `folder` 为 NULL、`journaled_seq` 为 0，在下一次同步（打开团队概览或任何写操作）
/// 时补建文件夹并补写历史日志。
const SCHEMA_V3: &str = r#"
ALTER TABLE roles ADD COLUMN folder TEXT;
ALTER TABLE roles ADD COLUMN folder_problem TEXT;
ALTER TABLE roles ADD COLUMN journaled_seq INTEGER NOT NULL DEFAULT 0;
"#;

/// v4：Harness 实例改为 ACP Agent（D13）。重建 `harnesses` 表：
/// - 旧 `kind`（claude_code / codex / mock）映射为同名预设；
/// - 启动命令：旧 `executable` 覆盖路径若有则沿用，否则用预设默认命令（`claude-code-acp`、`codex-acp`），
///   mock 为 `teamspace-mock-agent`；参数为预设默认（均为空）；
/// - 旧 `model`、`profile` 与 ACP 无对应字段，不再保留（历史活动记录中仍有）；
/// - 握手状态重置为 `unknown`。
///
/// 重建表期间外键检查由 [`migrate`] 暂时关闭，提交前做 `foreign_key_check`。
const SCHEMA_V4: &str = r#"
CREATE TABLE harnesses_v4 (
    id                   TEXT PRIMARY KEY,
    team_id              TEXT NOT NULL REFERENCES teams (id),
    name                 TEXT NOT NULL CHECK (name <> ''),
    preset               TEXT NOT NULL CHECK (preset IN ('claude_code', 'codex', 'minimax_code', 'gemini_cli', 'custom', 'mock')),
    command              TEXT NOT NULL CHECK (command <> ''),
    args                 TEXT NOT NULL DEFAULT '[]',
    env_profile          TEXT,
    handshake_status     TEXT NOT NULL DEFAULT 'unknown' CHECK (handshake_status IN ('connected', 'needs_auth', 'error', 'unknown')),
    protocol_version     INTEGER,
    agent_name           TEXT,
    agent_title          TEXT,
    agent_version        TEXT,
    capabilities         TEXT,
    auth_methods         TEXT NOT NULL DEFAULT '[]',
    handshake_detail     TEXT,
    handshake_checked_at TEXT,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    archived_at          TEXT
);
INSERT INTO harnesses_v4 (id, team_id, name, preset, command, args, created_at, updated_at, archived_at)
    SELECT id, team_id, name, kind,
           CASE
               WHEN kind = 'mock' THEN 'teamspace-mock-agent'
               WHEN executable IS NOT NULL AND trim(executable) <> '' THEN trim(executable)
               WHEN kind = 'claude_code' THEN 'claude-code-acp'
               ELSE 'codex-acp'
           END,
           '[]', created_at, updated_at, archived_at
    FROM harnesses;
DROP TABLE harnesses;
ALTER TABLE harnesses_v4 RENAME TO harnesses;
CREATE UNIQUE INDEX harnesses_active_name ON harnesses (team_id, name) WHERE archived_at IS NULL;
"#;

/// v5：执行（M2）。
///
/// - `runs`：每次执行一行；Harness 以快照保存（名称、预设、命令、参数）。
///   `runs_active_task` 保证同一任务同时只有一个活动执行；`runs_active_in_place` 保证同一个普通文件夹
///   工作空间同时只有一个原地执行（D15）。`pending_permission` 为等待 Human 的权限请求（JSON）。
/// - `run_updates`：执行进度。`seq` 是位置（时间顺序），`rev` 是本执行内的变更序号：条目被更新
///   （消息块合并、工具调用状态、权限决定）时 `rev` 前移，增量读取按 `rev` 游标进行。
///   `key` 用于按工具调用 ID、权限请求 ID 或计划找回同一条目。
const SCHEMA_V5: &str = r#"
CREATE TABLE runs (
    id                 TEXT PRIMARY KEY,
    team_id            TEXT NOT NULL REFERENCES teams (id),
    task_id            TEXT NOT NULL REFERENCES tasks (id),
    role_id            TEXT NOT NULL REFERENCES roles (id),
    harness_id         TEXT NOT NULL REFERENCES harnesses (id),
    harness_name       TEXT NOT NULL,
    harness_preset     TEXT NOT NULL,
    harness_command    TEXT NOT NULL,
    harness_args       TEXT NOT NULL DEFAULT '[]',
    attempt            INTEGER NOT NULL CHECK (attempt >= 1),
    status             TEXT NOT NULL CHECK (status IN ('queued', 'running', 'waiting_permission', 'succeeded', 'failed', 'cancelled', 'interrupted')),
    session_id         TEXT,
    workspace_path     TEXT NOT NULL,
    exec_dir           TEXT NOT NULL,
    isolation          TEXT NOT NULL CHECK (isolation IN ('worktree', 'in_place')),
    branch             TEXT,
    base_commit        TEXT,
    capabilities       TEXT,
    started_at         TEXT NOT NULL,
    ended_at           TEXT,
    stop_reason        TEXT,
    failure            TEXT,
    failure_kind       TEXT,
    summary            TEXT,
    changed_files      TEXT NOT NULL DEFAULT '[]',
    tool_call_count    INTEGER NOT NULL DEFAULT 0,
    taken_over         INTEGER NOT NULL DEFAULT 0 CHECK (taken_over IN (0, 1)),
    pending_permission TEXT,
    UNIQUE (task_id, attempt)
);
CREATE INDEX runs_task ON runs (task_id, attempt);
CREATE INDEX runs_role ON runs (role_id, started_at);
CREATE UNIQUE INDEX runs_active_task ON runs (task_id)
    WHERE status IN ('queued', 'running', 'waiting_permission');
CREATE UNIQUE INDEX runs_active_in_place ON runs (workspace_path)
    WHERE isolation = 'in_place' AND status IN ('queued', 'running', 'waiting_permission');

CREATE TABLE run_updates (
    seq        INTEGER PRIMARY KEY,
    id         TEXT NOT NULL UNIQUE,
    run_id     TEXT NOT NULL REFERENCES runs (id),
    rev        INTEGER NOT NULL,
    kind       TEXT NOT NULL,
    key        TEXT,
    body       TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (run_id, rev)
);
CREATE INDEX run_updates_key ON run_updates (run_id, key);
"#;

/// v6：新增原生 ACP 预设（CodeBuddy、Qoder CLI、Grok Build、Qwen Code、Kimi CLI、OpenCode）。
/// `harnesses.preset` 的 CHECK 约束不能原地修改，重建表（结构同 v4，仅扩充预设取值），数据原样保留。
const SCHEMA_V6: &str = r#"
CREATE TABLE harnesses_v6 (
    id                   TEXT PRIMARY KEY,
    team_id              TEXT NOT NULL REFERENCES teams (id),
    name                 TEXT NOT NULL CHECK (name <> ''),
    preset               TEXT NOT NULL CHECK (preset IN ('claude_code', 'codex', 'minimax_code', 'gemini_cli', 'codebuddy', 'qoder_cli', 'grok_build', 'qwen_code', 'kimi_cli', 'opencode', 'custom', 'mock')),
    command              TEXT NOT NULL CHECK (command <> ''),
    args                 TEXT NOT NULL DEFAULT '[]',
    env_profile          TEXT,
    handshake_status     TEXT NOT NULL DEFAULT 'unknown' CHECK (handshake_status IN ('connected', 'needs_auth', 'error', 'unknown')),
    protocol_version     INTEGER,
    agent_name           TEXT,
    agent_title          TEXT,
    agent_version        TEXT,
    capabilities         TEXT,
    auth_methods         TEXT NOT NULL DEFAULT '[]',
    handshake_detail     TEXT,
    handshake_checked_at TEXT,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    archived_at          TEXT
);
INSERT INTO harnesses_v6 SELECT * FROM harnesses;
DROP TABLE harnesses;
ALTER TABLE harnesses_v6 RENAME TO harnesses;
CREATE UNIQUE INDEX harnesses_active_name ON harnesses (team_id, name) WHERE archived_at IS NULL;
"#;

/// v7：执行记录增加快照比对得出的「范围外改动」与改动统计说明（S-1b）。
const SCHEMA_V7: &str = r#"
ALTER TABLE runs ADD COLUMN out_of_scope TEXT NOT NULL DEFAULT '[]';
ALTER TABLE runs ADD COLUMN change_note TEXT;
"#;

/// v8：执行记录增加「改动统计不完整」标记（快照降级，任务卡与执行面板据此提示）。
const SCHEMA_V8: &str = r#"
ALTER TABLE runs ADD COLUMN change_incomplete INTEGER NOT NULL DEFAULT 0;
"#;

/// v9：活动记录按岗位筛选（活动与审计页的筛选条件之一）。
/// 任务筛选已由 v2 的 `events_task (task_id, seq)` 覆盖，时间范围走 `events_team` 的区间扫描，
/// 只有「团队 + 岗位 + 倒序」这一组合缺少索引。
const SCHEMA_V9: &str = r#"
CREATE INDEX events_role ON events (team_id, role_id, seq);
"#;

const MIGRATIONS: &[&str] = &[
    SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6, SCHEMA_V7, SCHEMA_V8,
    SCHEMA_V9,
];

/// 打开数据库：WAL 日志、synchronous=NORMAL、开启外键，并迁移到当前版本。
pub(crate) fn open(path: &Path) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(Error::internal(format!(
            "无法启用 WAL 日志模式（当前：{mode}）"
        )));
    }
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 0 {
        return Err(Error::internal(format!(
            "数据库版本号异常（{version}），无法打开"
        )));
    }
    if version > SCHEMA_VERSION {
        return Err(Error::internal(format!(
            "数据库版本（{version}）高于当前程序支持的版本（{SCHEMA_VERSION}），请升级应用"
        )));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    // 重建表的迁移（v4）需要暂时关闭外键检查（SQLite 不允许在事务内切换），每步提交前自查一致性。
    conn.pragma_update(None, "foreign_keys", false)?;
    let result = (|| -> Result<()> {
        for (index, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            let broken: i64 =
                tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })?;
            if broken > 0 {
                return Err(Error::internal(format!(
                    "数据库迁移到版本 {} 后有 {broken} 处外键不一致，已回滚",
                    index + 1
                )));
            }
            tx.pragma_update(None, "user_version", index as i64 + 1)?;
            tx.commit()?;
        }
        Ok(())
    })();
    conn.pragma_update(None, "foreign_keys", true)?;
    result
}

/// 当前 UTC 时间，RFC3339，固定毫秒精度（保证字符串排序即时间排序）。
pub(crate) fn now() -> String {
    let t = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
        t.millisecond()
    )
}

/// 新的 UUID v7（按毫秒时间有序；需要严格写入顺序时使用自增序号，如活动记录的 `seq`）。
pub(crate) fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// 枚举的 snake_case 字符串形式（与 IPC 一致）。
pub(crate) fn enum_str<T: Serialize>(value: T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        other => unreachable!("enum must serialize as string: {other:?}"),
    }
}

/// 从数据库列读取枚举。
pub(crate) fn enum_col<T: DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    idx: usize,
) -> rusqlite::Result<T> {
    let raw: String = row.get(idx)?;
    serde_json::from_value(serde_json::Value::String(raw)).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(err))
    })
}

/// 去掉首尾空白，空串视为未填写。
pub(crate) fn non_blank(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_to_current_version_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let conn = open(&path).unwrap();
        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        let fk: i64 = conn
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        drop(conn);
        open(&path).unwrap();
    }

    #[test]
    fn rejects_newer_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
                .unwrap();
        }
        let err = open(&path).unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Internal);
    }

    #[test]
    fn rejects_negative_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", -1).unwrap();
        }
        let err = open(&path).unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Internal);
        assert!(err.to_string().contains("-1"), "{err}");
    }

    #[test]
    fn uses_wal_and_normal_sync() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("t.db")).unwrap();
        let mode: String = conn
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        let sync: i64 = conn
            .pragma_query_value(None, "synchronous", |r| r.get(0))
            .unwrap();
        assert_eq!(sync, 1, "NORMAL");
    }

    #[test]
    fn v1_events_migrate_to_v2_in_id_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute_batch(
                "INSERT INTO teams VALUES ('t', 'T', '', '/w', 'a', 'a', NULL);
                 INSERT INTO events VALUES ('e2', 't', NULL, NULL, 'team_created', 'human', 's2', '{}', 'b');
                 INSERT INTO events VALUES ('e1', 't', NULL, NULL, 'team_created', 'human', 's1', '{}', 'a');
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        }
        let conn = open(&path).unwrap();
        let rows: Vec<(i64, String)> = conn
            .prepare("SELECT seq, id FROM events ORDER BY seq")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(rows, vec![(1, "e1".to_owned()), (2, "e2".to_owned())]);
    }

    #[test]
    fn v2_roles_migrate_to_v3_without_folders() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute_batch(SCHEMA_V2).unwrap();
            conn.execute_batch(
                "INSERT INTO teams VALUES ('t', 'T', '', '/w', 'a', 'a', NULL);
                 INSERT INTO roles (id, team_id, name, code, task_prefix, created_at, updated_at)
                     VALUES ('r', 't', '前端', 'role_fe', 'FE', 'a', 'a');
                 PRAGMA user_version = 2;",
            )
            .unwrap();
        }
        let conn = open(&path).unwrap();
        let row: (Option<String>, Option<String>, i64) = conn
            .query_row(
                "SELECT folder, folder_problem, journaled_seq FROM roles WHERE id = 'r'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, (None, None, 0));
    }

    #[test]
    fn v3_harnesses_migrate_to_acp_agents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute_batch(SCHEMA_V2).unwrap();
            conn.execute_batch(SCHEMA_V3).unwrap();
            conn.execute_batch(
                "INSERT INTO teams VALUES ('t', 'T', '', '/w', 'a', 'a', NULL);
                 INSERT INTO harnesses (id, team_id, name, kind, executable, model, profile,
                                        health_status, health_version, health_checked_at,
                                        created_at, updated_at)
                     VALUES ('h1', 't', 'claude', 'claude_code', NULL, 'opus', '/s.json',
                             'ok', '2.1.0', 'b', 'a', 'a'),
                            ('h2', 't', 'codex', 'codex', ' /opt/bin/codex ', NULL, NULL,
                             'error', NULL, 'b', 'a', 'a'),
                            ('h3', 't', 'mock', 'mock', NULL, NULL, NULL,
                             'unknown', NULL, NULL, 'a', 'a'),
                            ('h4', 't', 'old', 'codex', NULL, NULL, NULL,
                             'unknown', NULL, NULL, 'a', 'a');
                 UPDATE harnesses SET archived_at = 'c' WHERE id = 'h4';
                 INSERT INTO roles (id, team_id, name, code, task_prefix, harness_id, created_at, updated_at)
                     VALUES ('r', 't', '前端', 'role_fe', 'FE', 'h1', 'a', 'a');
                 PRAGMA user_version = 3;",
            )
            .unwrap();
        }
        let conn = open(&path).unwrap();
        let rows: Vec<(String, String, String, String, String, Option<String>)> = conn
            .prepare(
                "SELECT id, preset, command, args, handshake_status, archived_at
                 FROM harnesses ORDER BY id",
            )
            .unwrap()
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let s = |v: &str| v.to_owned();
        assert_eq!(
            rows,
            vec![
                (
                    s("h1"),
                    s("claude_code"),
                    s("claude-code-acp"),
                    s("[]"),
                    s("unknown"),
                    None
                ),
                (
                    s("h2"),
                    s("codex"),
                    s("/opt/bin/codex"),
                    s("[]"),
                    s("unknown"),
                    None
                ),
                (
                    s("h3"),
                    s("mock"),
                    s("teamspace-mock-agent"),
                    s("[]"),
                    s("unknown"),
                    None
                ),
                (
                    s("h4"),
                    s("codex"),
                    s("codex-acp"),
                    s("[]"),
                    s("unknown"),
                    Some(s("c"))
                ),
            ]
        );
        // 绑定关系保留，外键仍指向新表且仍然生效。
        let bound: String = conn
            .query_row("SELECT harness_id FROM roles WHERE id = 'r'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(bound, "h1");
        let fk: i64 = conn
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let err = conn
            .execute("UPDATE roles SET harness_id = 'missing' WHERE id = 'r'", [])
            .unwrap_err();
        assert!(err.to_string().contains("FOREIGN KEY"), "{err}");
        // 活动名唯一索引重建了：未归档同名冲突，已归档的不冲突。
        assert!(
            conn.execute(
                "INSERT INTO harnesses (id, team_id, name, preset, command, created_at, updated_at)
                 VALUES ('h5', 't', 'claude', 'custom', 'x', 'a', 'a')",
                [],
            )
            .is_err()
        );
        conn.execute(
            "INSERT INTO harnesses (id, team_id, name, preset, command, created_at, updated_at)
             VALUES ('h6', 't', 'old', 'gemini_cli', 'gemini', 'a', 'a')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn v4_database_migrates_to_v5_with_run_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..4] {
                conn.execute_batch(sql).unwrap();
            }
            conn.execute_batch(
                "INSERT INTO teams VALUES ('t', 'T', '', '/w', 'a', 'a', NULL);
                 PRAGMA user_version = 4;",
            )
            .unwrap();
        }
        let conn = open(&path).unwrap();
        for table in ["runs", "run_updates"] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{table}");
        }
        let teams: i64 = conn
            .query_row("SELECT COUNT(*) FROM teams", [], |r| r.get(0))
            .unwrap();
        assert_eq!(teams, 1);
    }

    #[test]
    fn v5_database_migrates_to_v6_with_new_presets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..5] {
                conn.execute_batch(sql).unwrap();
            }
            conn.execute_batch(
                "INSERT INTO teams VALUES ('t', 'T', '', '/w', 'a', 'a', NULL);
                 INSERT INTO harnesses (id, team_id, name, preset, command, args, created_at, updated_at)
                     VALUES ('h1', 't', 'claude', 'claude_code', 'claude-code-acp', '[]', 'a', 'a'),
                            ('h2', 't', 'old', 'mock', 'teamspace-mock-agent', '[]', 'a', 'a');
                 UPDATE harnesses SET archived_at = 'c' WHERE id = 'h2';
                 INSERT INTO roles (id, team_id, name, code, task_prefix, harness_id, created_at, updated_at)
                     VALUES ('r', 't', '前端', 'role_fe', 'FE', 'h1', 'a', 'a');
                 PRAGMA user_version = 5;",
            )
            .unwrap();
        }
        let conn = open(&path).unwrap();
        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        let fk: i64 = conn
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM harnesses", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
        let role_harness: String = conn
            .query_row("SELECT harness_id FROM roles WHERE id = 'r'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(role_harness, "h1");
        // 新预设可以写入；未知预设仍被拒绝。
        for preset in [
            "codebuddy",
            "qoder_cli",
            "grok_build",
            "qwen_code",
            "kimi_cli",
            "opencode",
        ] {
            conn.execute(
                "INSERT INTO harnesses (id, team_id, name, preset, command, created_at, updated_at)
                 VALUES (?1, 't', ?1, ?2, 'x', 'a', 'a')",
                [preset, preset],
            )
            .unwrap_or_else(|e| panic!("{preset}: {e}"));
        }
        assert!(
            conn.execute(
                "INSERT INTO harnesses (id, team_id, name, preset, command, created_at, updated_at)
                 VALUES ('bad', 't', 'bad', 'code_buddy', 'x', 'a', 'a')",
                [],
            )
            .is_err()
        );
        // 活动名唯一索引仍然有效。
        assert!(
            conn.execute(
                "INSERT INTO harnesses (id, team_id, name, preset, command, created_at, updated_at)
                 VALUES ('dup', 't', 'claude', 'custom', 'x', 'a', 'a')",
                [],
            )
            .is_err()
        );
    }

    #[test]
    fn timestamps_have_fixed_width() {
        let t = now();
        assert_eq!(t.len(), "2026-10-12T06:06:01.262Z".len());
        assert!(t.ends_with('Z'));
    }
}
