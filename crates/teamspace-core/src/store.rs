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
pub(crate) const SCHEMA_VERSION: i64 = 18;

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

/// v9：验收（M3-01，D20）。
///
/// - `base_branch`：worktree 执行开始时工作空间所在的分支（验收通过时合并进这个分支）；
/// - `merged_commit`：验收通过、合并成功后基线分支上的合并提交；
/// - `revision_of`：退回后续接的执行指向被退回的那次执行（同一执行目录、同一分支）；
/// - `review_outcome` / `review_comment` / `reviewed_at`：这次执行的验收结果（approved / rejected /
///   conflict）、退回意见与时间。
const SCHEMA_V9: &str = r#"
ALTER TABLE runs ADD COLUMN base_branch TEXT;
ALTER TABLE runs ADD COLUMN merged_commit TEXT;
ALTER TABLE runs ADD COLUMN revision_of TEXT REFERENCES runs (id);
ALTER TABLE runs ADD COLUMN review_outcome TEXT
    CHECK (review_outcome IS NULL OR review_outcome IN ('approved', 'rejected', 'conflict'));
ALTER TABLE runs ADD COLUMN review_comment TEXT;
ALTER TABLE runs ADD COLUMN reviewed_at TEXT;
"#;

/// v10：活动记录按岗位筛选（活动与审计页的筛选条件之一）。
/// 任务筛选已由 v2 的 `events_task (task_id, seq)` 覆盖，时间范围走 `events_team` 的区间扫描，
/// 只有「团队 + 岗位 + 倒序」这一组合缺少索引。
const SCHEMA_V10: &str = r#"
CREATE INDEX events_role ON events (team_id, role_id, seq);
"#;

/// v11：验收合并的「合并中」标记（M3-01 修复轮）。在调用 Git 合并之前写入：目标基线分支、执行分支预期的
/// head 与开始时间；合并结果落库时清除。应用在合并之后、落库之前退出（或落库失败）时标记会留下，
/// 打开验收页、再次通过或重启时据此对账：执行分支已并入基线就补记通过，否则清除标记回到待验收。
/// `conflict_files`：合并冲突时的冲突文件（JSON 数组），验收页刷新后据此列出。
const SCHEMA_V11: &str = r#"
ALTER TABLE runs ADD COLUMN merge_base_branch TEXT;
ALTER TABLE runs ADD COLUMN merge_head TEXT;
ALTER TABLE runs ADD COLUMN merging_at TEXT;
ALTER TABLE runs ADD COLUMN conflict_files TEXT NOT NULL DEFAULT '[]';
"#;

/// v12：执行记录的「成功但无改动」标记与中断时的改动保全信息（使用记录 1、2）。
/// `no_changes`：执行成功但相对基线没有任何改动（worktree 是执行分支相对基线提交的 diff 为空，原地执行是
/// 快照比对没有发现改动）。`interrupted_commit`：中断时把执行目录里的未提交改动提交到执行分支得到的提交。
/// `interrupted_problem`：没能保全时的原因（如执行目录已不在执行分支上）。
const SCHEMA_V12: &str = r#"
ALTER TABLE runs ADD COLUMN no_changes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE runs ADD COLUMN interrupted_commit TEXT;
ALTER TABLE runs ADD COLUMN interrupted_problem TEXT;
"#;

/// v13：岗位通信的消息与线程（M5-01）。
///
/// 团队通信经 Teamspace 中转并持久化（不以 Harness 之间的点对点通信作为正式渠道）。
/// - `message_threads`：一次往来的主题。`participants` 存参与方（JSON 数组），`updated_at` 随最近一条
///   消息刷新，线程列表按它倒序。
/// - `messages`：线程里的每一条消息。`sender_kind` 为 human / leader / role，`sender_role_id` 只在发送方
///   是岗位时有；`status` 为 queued（排队）→ delivering（投递中）→ replied（已回复）/ failed（失败）/
///   expired（过期），`failure_reason` 与 `reply_body` 分别记失败原因与回复正文。
///   应用重启时残留的 delivering 会退回 queued（见 `service::message::requeue_delivering`）。
const SCHEMA_V13: &str = r#"
CREATE TABLE message_threads (
    id           TEXT PRIMARY KEY,
    team_id      TEXT NOT NULL REFERENCES teams (id),
    task_id      TEXT REFERENCES tasks (id),
    subject      TEXT NOT NULL CHECK (subject <> ''),
    participants TEXT NOT NULL DEFAULT '[]',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE INDEX message_threads_team ON message_threads (team_id, updated_at);

CREATE TABLE messages (
    id             TEXT PRIMARY KEY,
    thread_id      TEXT NOT NULL REFERENCES message_threads (id),
    team_id        TEXT NOT NULL REFERENCES teams (id),
    task_id        TEXT REFERENCES tasks (id),
    sender_kind    TEXT NOT NULL CHECK (sender_kind IN ('human', 'leader', 'role')),
    sender_role_id TEXT REFERENCES roles (id),
    to_role_id     TEXT NOT NULL REFERENCES roles (id),
    body           TEXT NOT NULL CHECK (body <> ''),
    status         TEXT NOT NULL
                   CHECK (status IN ('queued', 'delivering', 'replied', 'failed', 'expired')),
    failure_reason TEXT,
    reply_body     TEXT,
    created_at     TEXT NOT NULL,
    delivered_at   TEXT,
    replied_at     TEXT
);
CREATE INDEX messages_thread ON messages (thread_id, created_at);
CREATE INDEX messages_to_role ON messages (to_role_id, status, created_at);
CREATE INDEX messages_status ON messages (status, created_at);
"#;

/// v14：决策记录与应用内通知的订阅设置（M5-02）。
///
/// - `decisions`：Human 拍板的决策。结构化保存，供 `list_decisions` 查询；同一决策还会写一条
///   `decision_recorded` 活动，岗位工作日志据此追加一行「决策：…」。
/// - `notification_settings`：通知订阅。`scope` 为 `*` 表示该类别的总开关，否则是岗位 id；
///   没有记录时默认开启，因此这张表只存「关掉」或「显式打开」的选择。
const SCHEMA_V14: &str = r#"
CREATE TABLE decisions (
    id           TEXT PRIMARY KEY,
    team_id      TEXT NOT NULL REFERENCES teams (id),
    task_id      TEXT REFERENCES tasks (id),
    message_id   TEXT REFERENCES messages (id),
    role_id      TEXT REFERENCES roles (id),
    text         TEXT NOT NULL CHECK (text <> ''),
    resumed_task INTEGER NOT NULL DEFAULT 0 CHECK (resumed_task IN (0, 1)),
    created_at   TEXT NOT NULL
);
CREATE INDEX decisions_team ON decisions (team_id, created_at);
CREATE INDEX decisions_task ON decisions (task_id);
CREATE INDEX decisions_message ON decisions (message_id);

CREATE TABLE notification_settings (
    team_id  TEXT NOT NULL REFERENCES teams (id),
    category TEXT NOT NULL,
    scope    TEXT NOT NULL,
    enabled  INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    PRIMARY KEY (team_id, category, scope)
);
"#;

/// v15：岗位换绑与消息绑定快照（M6-01）。
///
/// - `role_pending_bindings`：待生效的换绑。一个岗位最多一条（唯一索引），换绑生效或撤销后删除；
///   换绑生效后岗位的当前绑定（`roles.harness_id`）指向目标实例。
/// - `messages.binding_harness_id`：消息入队时接收岗位绑定的 Harness 实例 id（快照，仅留痕）。
///   实际投递按岗位「当前有效」绑定解析，快照只用于事后追溯。
const SCHEMA_V15: &str = r#"
CREATE TABLE role_pending_bindings (
    id            TEXT PRIMARY KEY,
    role_id       TEXT NOT NULL REFERENCES roles (id),
    team_id       TEXT NOT NULL REFERENCES teams (id),
    harness_id    TEXT NOT NULL REFERENCES harnesses (id),
    mode          TEXT NOT NULL
                  CHECK (mode IN ('after_current', 'cancel_and_rerun', 'manual_takeover')),
    active_run_id TEXT,
    requested_at  TEXT NOT NULL,
    created_at    TEXT NOT NULL
);
CREATE UNIQUE INDEX role_pending_bindings_role ON role_pending_bindings (role_id);

ALTER TABLE messages ADD COLUMN binding_harness_id TEXT;
"#;

/// v16：消息的入队时间（M5-06 回归修复）。
///
/// 过期判定改用「最近一次入队时间」：重发一条已经过期的消息时，`created_at` 仍是很久以前，
/// 按它判过期会让重发的消息立刻再次过期。新增 `queued_at` 列，入队与重发时写当前时间；
/// 迁移时用 `created_at` 回填（旧数据没有「入队」概念，按创建时间算）。
const SCHEMA_V16: &str = r#"
ALTER TABLE messages ADD COLUMN queued_at TEXT NOT NULL DEFAULT '';
UPDATE messages SET queued_at = created_at;
"#;

/// v17：待生效换绑记录发起时的旧绑定（M6-04）。
///
/// `role_pending_bindings.previous_harness_id` / `previous_harness_name`：发起换绑时岗位的当前绑定。
/// 之前界面靠岗位当前绑定反推「换绑前绑的是哪个」，岗位绑定在别处改过时就会显示错的旧实例名。
/// 迁移时用岗位当前的绑定回填（旧记录没有这个概念，按当时的绑定算）。
const SCHEMA_V17: &str = r#"
ALTER TABLE role_pending_bindings ADD COLUMN previous_harness_id TEXT;
ALTER TABLE role_pending_bindings ADD COLUMN previous_harness_name TEXT;
UPDATE role_pending_bindings SET
    previous_harness_id = (
        SELECT r.harness_id FROM roles r WHERE r.id = role_pending_bindings.role_id
    ),
    previous_harness_name = (
        SELECT h.name FROM harnesses h
        WHERE h.id = (SELECT r.harness_id FROM roles r WHERE r.id = role_pending_bindings.role_id)
    );
"#;

/// v18：1 万条任务与事件时的查询索引（M7-03 性能）。
///
/// - `tasks_team_status_updated`：看板按状态分页（每列按更新时间倒序取一页）与团队概览的
///   「进行中的任务」都按 `(团队, 状态, 更新时间, id)` 取，没有它每列都要扫全表再排序。
/// - `tasks_team_updated`：轻量任务列表（活动页的任务下拉框）按更新时间倒序取，
///   没有它要先把团队的全部任务排一遍。
/// - `tasks_owner_updated`：岗位工作台取该岗位最近的任务，同样按更新时间倒序。
/// - `events_team_kind_seq`：活动按种类筛选（团队 + 种类 + 序号），以及按种类找单条活动
///   （工作日志、执行摘要、越权记录）。列顺序让「按种类倒序取一页」不用再排序，
///   也不会为了找一个冷门种类把该团队的活动整张表扫一遍。
/// - `events_team_task_seq`：活动按任务筛选（团队 + 任务）。
/// - `message_threads_team` 加上 `id`：线程列表按 `(更新时间, id)` 倒序，取完不用再排序。
const SCHEMA_V18: &str = r#"
CREATE INDEX tasks_team_status_updated ON tasks (team_id, status, updated_at DESC, id DESC);
CREATE INDEX tasks_team_updated ON tasks (team_id, updated_at DESC, id DESC);
CREATE INDEX tasks_owner_updated ON tasks (owner_role_id, updated_at DESC, id DESC);
CREATE INDEX events_team_kind_seq ON events (team_id, kind, seq);
CREATE INDEX events_team_task_seq ON events (team_id, task_id, seq);
DROP INDEX message_threads_team;
CREATE INDEX message_threads_team ON message_threads (team_id, updated_at DESC, id DESC);
"#;

const MIGRATIONS: &[&str] = &[
    SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6, SCHEMA_V7, SCHEMA_V8,
    SCHEMA_V9, SCHEMA_V10, SCHEMA_V11, SCHEMA_V12, SCHEMA_V13, SCHEMA_V14, SCHEMA_V15, SCHEMA_V16,
    SCHEMA_V17, SCHEMA_V18,
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
    fn v13_adds_message_tables_and_keeps_existing_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            // 先建到 v12：只跑前 12 段迁移。
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "foreign_keys", false).unwrap();
            for sql in MIGRATIONS.iter().take(12) {
                conn.execute_batch(sql).unwrap();
            }
            conn.execute_batch(
                "INSERT INTO teams (id, name, description, workspace_path, created_at, updated_at)
                 VALUES ('t1', 'T', '', '/w', 'a', 'a');
                 PRAGMA user_version = 12;",
            )
            .unwrap();
        }
        // 打开时迁移到 v13。
        let conn = open(&path).unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        // 旧数据还在。
        let name: String = conn
            .query_row("SELECT name FROM teams WHERE id = 't1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "T");
        // 新表在，旧版本没有它们。
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name IN ('messages', 'message_threads')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 2);
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
    fn v8_runs_migrate_to_v9_with_review_columns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..8] {
                conn.execute_batch(sql).unwrap();
            }
            conn.execute_batch(
                "INSERT INTO teams VALUES ('t', 'T', '', '/w', 'a', 'a', NULL);
                 INSERT INTO harnesses (id, team_id, name, preset, command, created_at, updated_at)
                     VALUES ('h', 't', 'mock', 'mock', 'x', 'a', 'a');
                 INSERT INTO roles (id, team_id, name, code, task_prefix, harness_id, created_at, updated_at)
                     VALUES ('r', 't', '前端', 'role_fe', 'FE', 'h', 'a', 'a');
                 INSERT INTO tasks (id, team_id, key, title, status, owner_role_id, created_at, updated_at)
                     VALUES ('k', 't', 'FE-001', '页', 'review', 'r', 'a', 'a');
                 INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                                   harness_command, attempt, status, workspace_path, exec_dir, isolation,
                                   branch, base_commit, started_at)
                     VALUES ('run', 't', 'k', 'r', 'h', 'mock', 'mock', 'x', 1, 'succeeded', '/w', '/x',
                             'worktree', 'teamspace/FE-001-r1', 'abc', 'a');
                 PRAGMA user_version = 8;",
            )
            .unwrap();
        }
        let conn = open(&path).unwrap();
        let row: (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
        ) = conn
            .query_row(
                "SELECT base_branch, merged_commit, revision_of, review_outcome, branch FROM runs",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            (None, None, None, None, "teamspace/FE-001-r1".to_owned())
        );
        assert!(
            conn.execute("UPDATE runs SET review_outcome = 'maybe'", [])
                .is_err()
        );
        conn.execute("UPDATE runs SET review_outcome = 'rejected'", [])
            .unwrap();
    }

    #[test]
    fn v9_database_migrates_to_v10_with_the_events_role_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..9] {
                conn.execute_batch(sql).unwrap();
            }
            conn.execute_batch(
                "INSERT INTO teams VALUES ('t', 'T', '', '/w', 'a', 'a', NULL);
                 INSERT INTO roles (id, team_id, name, code, task_prefix, created_at, updated_at)
                     VALUES ('r', 't', '前端', 'role_fe', 'FE', 'a', 'a');
                 INSERT INTO events (id, team_id, task_id, role_id, kind, actor, summary, data, created_at)
                     VALUES ('e', 't', NULL, 'r', 'team_created', 'human', 's', '{}', 'a');
                 PRAGMA user_version = 9;",
            )
            .unwrap();
        }
        let conn = open(&path).unwrap();
        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        {
            let mut stmt = conn
                .prepare(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'events'",
                )
                .unwrap();
            let indexes: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert!(indexes.contains(&"events_role".to_owned()), "{indexes:?}");
        }
        // v9 的验收列原样保留，索引迁移不动 runs 表。
        let columns: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('runs') WHERE name = 'review_outcome'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(columns, 1);
        // 重复打开是幂等的（migrate 到当前版本后直接返回）。
        drop(conn);
        let conn = open(&path).unwrap();
        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn v10_runs_migrate_to_v11_with_merge_marker_columns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..10] {
                conn.execute_batch(sql).unwrap();
            }
            conn.execute_batch("PRAGMA user_version = 10;").unwrap();
        }
        let conn = open(&path).unwrap();
        for column in [
            "merge_base_branch",
            "merge_head",
            "merging_at",
            "conflict_files",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('runs') WHERE name = ?1",
                    [column],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{column}");
        }
    }

    #[test]
    fn timestamps_have_fixed_width() {
        let t = now();
        assert_eq!(t.len(), "2026-10-12T06:06:01.262Z".len());
        assert!(t.ends_with('Z'));
    }

    /// v15：待生效换绑表与消息绑定快照列。
    #[test]
    fn v14_migrates_to_v15_with_pending_bindings_and_message_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..14] {
                conn.execute_batch(sql).unwrap();
            }
            conn.execute_batch("PRAGMA user_version = 14;").unwrap();
        }
        let conn = open(&path).unwrap();
        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        for (table, columns) in [
            (
                "role_pending_bindings",
                [
                    "id",
                    "role_id",
                    "team_id",
                    "harness_id",
                    "mode",
                    "active_run_id",
                    "requested_at",
                    "created_at",
                ]
                .as_slice(),
            ),
            ("messages", ["binding_harness_id"].as_slice()),
        ] {
            for column in columns {
                let n: i64 = conn
                    .query_row(
                        &format!(
                            "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"
                        ),
                        [column],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(n, 1, "{table}.{column}");
            }
        }
        // 一个岗位最多一条待生效换绑。
        conn.execute_batch(
            "INSERT INTO teams (id, name, workspace_path, created_at, updated_at)
             VALUES ('t1', '团队', '/tmp', '2026-10-12T00:00:00.000Z', '2026-10-12T00:00:00.000Z');
             INSERT INTO roles (id, team_id, name, code, task_prefix, description, guidelines,
                                is_leader, harness_id, created_at, updated_at)
             VALUES ('r1', 't1', '前端', 'role_fe', 'FE', '', '', 0, NULL,
                     '2026-10-12T00:00:00.000Z', '2026-10-12T00:00:00.000Z');
             INSERT INTO harnesses (id, team_id, name, preset, command, args, env_profile,
                                    handshake_status, created_at, updated_at)
             VALUES ('h1', 't1', 'Mock', 'mock', 'x', '[]', NULL, 'unknown',
                     '2026-10-12T00:00:00.000Z', '2026-10-12T00:00:00.000Z');",
        )
        .unwrap();
        let insert = "INSERT INTO role_pending_bindings
            (id, role_id, team_id, harness_id, mode, active_run_id, requested_at, created_at)
            VALUES (?1, 'r1', 't1', 'h1', 'after_current', NULL,
                    '2026-10-12T00:00:00.000Z', '2026-10-12T00:00:00.000Z')";
        conn.execute(insert, ["p1"]).unwrap();
        let err = conn.execute(insert, ["p2"]).unwrap_err();
        assert!(err.to_string().contains("UNIQUE"), "{err}");
    }

    /// v13 的库升到 v14：决策表与通知订阅表建好，老数据不受影响。
    #[test]
    fn v13_migrates_to_v14_with_decision_and_notification_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..13] {
                conn.execute_batch(sql).unwrap();
            }
            conn.execute_batch("PRAGMA user_version = 13;").unwrap();
        }
        let conn = open(&path).unwrap();
        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        for (table, columns) in [
            (
                "decisions",
                [
                    "id",
                    "team_id",
                    "task_id",
                    "message_id",
                    "role_id",
                    "text",
                    "resumed_task",
                    "created_at",
                ]
                .as_slice(),
            ),
            (
                "notification_settings",
                ["team_id", "category", "scope", "enabled"].as_slice(),
            ),
        ] {
            for column in columns {
                let n: i64 = conn
                    .query_row(
                        &format!(
                            "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"
                        ),
                        [column],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(n, 1, "{table}.{column}");
            }
        }
        // 订阅设置一行一个（团队，类别，作用域）：重复写入是替换而不是新增。
        conn.execute_batch(
            "INSERT INTO teams (id, name, workspace_path, created_at, updated_at)
             VALUES ('t1', '团队', '/tmp', '2026-10-12T00:00:00.000Z', '2026-10-12T00:00:00.000Z');",
        )
        .unwrap();
        for _ in 0..2 {
            conn.execute(
                "INSERT INTO notification_settings (team_id, category, scope, enabled)
                 VALUES ('t1', 'task_review', '*', 0)
                 ON CONFLICT (team_id, category, scope) DO UPDATE SET enabled = 0",
                [],
            )
            .unwrap();
        }
        let rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM notification_settings WHERE team_id = 't1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, 1);
    }

    /// v15 的库升到 v16：消息多出入队时间列，老消息按创建时间回填。
    #[test]
    fn v15_migrates_to_v16_with_the_message_queue_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..15] {
                conn.execute_batch(sql).unwrap();
            }
            conn.execute_batch(
                "PRAGMA user_version = 15;
                 INSERT INTO teams (id, name, workspace_path, created_at, updated_at)
                 VALUES ('t1', '团队', '/tmp', '2026-10-12T00:00:00.000Z', '2026-10-12T00:00:00.000Z');
                 INSERT INTO roles (id, team_id, name, code, task_prefix, description, guidelines,
                                    is_leader, harness_id, created_at, updated_at)
                 VALUES ('r1', 't1', '前端', 'role_fe', 'FE', '', '', 0, NULL,
                         '2026-10-12T00:00:00.000Z', '2026-10-12T00:00:00.000Z');
                 INSERT INTO message_threads (id, team_id, task_id, subject, participants,
                                             created_at, updated_at)
                 VALUES ('th1', 't1', NULL, '进度', '[]',
                         '2026-10-12T00:00:00.000Z', '2026-10-12T00:00:00.000Z');
                 INSERT INTO messages (id, thread_id, team_id, task_id, sender_kind, sender_role_id,
                                       to_role_id, body, status, failure_reason, reply_body,
                                       created_at, delivered_at, replied_at)
                 VALUES ('m1', 'th1', 't1', NULL, 'human', NULL, 'r1', '进度如何', 'queued',
                         NULL, NULL, '2026-10-12T00:00:00.000Z', NULL, NULL);",
            )
            .unwrap();
        }
        let conn = open(&path).unwrap();
        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name = 'queued_at'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "messages.queued_at");
        // 老消息没有入队概念，按创建时间回填。
        let (created_at, queued_at): (String, String) = conn
            .query_row(
                "SELECT created_at, queued_at FROM messages WHERE id = 'm1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(queued_at, created_at);
    }

    /// v16 的库升到 v17：待生效换绑补上发起时的旧绑定，老记录按岗位当前绑定回填。
    #[test]
    fn v16_migrates_to_v17_with_the_pending_binding_previous_harness() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..16] {
                conn.execute_batch(sql).unwrap();
            }
            conn.execute_batch(
                "PRAGMA user_version = 16;
                 INSERT INTO teams (id, name, workspace_path, created_at, updated_at)
                 VALUES ('t1', '团队', '/tmp', '2026-10-12T00:00:00.000Z', '2026-10-12T00:00:00.000Z');
                 INSERT INTO harnesses (id, team_id, name, preset, command, args, created_at, updated_at)
                 VALUES ('h1', 't1', '旧实例', 'mock', 'x', '[]',
                         '2026-10-12T00:00:00.000Z', '2026-10-12T00:00:00.000Z');
                 INSERT INTO roles (id, team_id, name, code, task_prefix, description, guidelines,
                                    is_leader, harness_id, created_at, updated_at)
                 VALUES ('r1', 't1', '前端', 'role_fe', 'FE', '', '', 0, 'h1',
                         '2026-10-12T00:00:00.000Z', '2026-10-12T00:00:00.000Z');
                 INSERT INTO role_pending_bindings (id, role_id, team_id, harness_id, mode,
                                                    active_run_id, requested_at, created_at)
                 VALUES ('p1', 'r1', 't1', 'h1', 'after_current', NULL,
                         '2026-10-12T00:00:00.000Z', '2026-10-12T00:00:00.000Z');",
            )
            .unwrap();
        }
        let conn = open(&path).unwrap();
        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        let (previous_id, previous_name): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT previous_harness_id, previous_harness_name
                 FROM role_pending_bindings WHERE id = 'p1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        // 老记录没有「发起时的绑定」这个概念，按岗位当时的当前绑定回填。
        assert_eq!(previous_id.as_deref(), Some("h1"));
        assert_eq!(previous_name.as_deref(), Some("旧实例"));
    }

    /// v17 的库升到 v18：补上查询索引，看板与活动筛选不再扫全表。
    #[test]
    fn v17_migrates_to_v18_with_the_query_indexes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..17] {
                conn.execute_batch(sql).unwrap();
            }
            conn.execute_batch("PRAGMA user_version = 17;").unwrap();
        }
        let conn = open(&path).unwrap();
        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);

        let names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index'")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        for index in [
            "tasks_team_status_updated",
            "tasks_team_updated",
            "tasks_owner_updated",
            "events_team_kind_seq",
            "events_team_task_seq",
            // 线程索引重建过：带上了 id 这一列的次序键。
            "message_threads_team",
        ] {
            assert!(
                names.iter().any(|name| name == index),
                "缺少索引 {index}：{names:?}"
            );
        }
        // 重建后的线程索引列序是 (team_id, updated_at, id)。
        let columns: Vec<String> = conn
            .prepare("PRAGMA index_info(message_threads_team)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(2))
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(columns, ["team_id", "updated_at", "id"]);
    }
}
