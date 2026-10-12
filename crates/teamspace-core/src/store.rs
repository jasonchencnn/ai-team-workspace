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
pub(crate) const SCHEMA_VERSION: i64 = 2;

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

const MIGRATIONS: &[&str] = &[SCHEMA_V1, SCHEMA_V2];

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
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
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
    fn timestamps_have_fixed_width() {
        let t = now();
        assert_eq!(t.len(), "2026-10-12T06:06:01.262Z".len());
        assert!(t.ends_with('Z'));
    }
}
