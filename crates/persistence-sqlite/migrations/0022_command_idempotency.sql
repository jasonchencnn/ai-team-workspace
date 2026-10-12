-- ===========================================================================
-- 0022_command_idempotency.sql
--
-- DEV-02-14：为 DATA_CONVENTIONS §4 的幂等规则建记录表。
--
-- 规格原文（DATA_CONVENTIONS §4）：
--
--   幂等记录键为 `(team_id, actor_fingerprint, command_name, idempotency_key)`，
--   保存 payload hash、状态和首次结果引用。
--
-- §4 的四种情况与本表字段的对应：
--
--   | 情况                                   | 字段落点                          |
--   |----------------------------------------|-----------------------------------|
--   | 同 key + 同 payload，首次仍处理中       | state=IN_PROGRESS                |
--   | 同 key + 同 payload，首次成功          | state=SUCCEEDED + result_ref     |
--   | 同 key + 同 payload，首次稳定业务失败  | state=FAILED + first_error_code  |
--   | 同 key + 不同 payload                 | UNIQUE 冲突 → 应用层报 CONCURRENCY_IDEMPOTENCY_CONFLICT |
--   | key 已超保留期                         | 不在本表的状态机内，见下方说明    |
--
-- 本迁移**只新增一张表**，不改写 0001～0021 任何一行（SQLx checksum 不得漂移）。
--
-- ---------------------------------------------------------------------------
-- 为什么第四种情况靠 UNIQUE 冲突而不是表内状态
--
--   §4 写的是「同 key + 不同 payload → CONCURRENCY_IDEMPOTENCY_CONFLICT」，
--   这**不是**一次状态迁移，而是「另一个逻辑操作复用了别人的 key」。
--   让它成为 UNIQUE 约束的违反，是唯一能让数据库拒绝它的方式：
--   同一 (team_id, actor_fingerprint, command_name, idempotency_key)
--   在任何时刻只能有一行。要写第二行就必须先删第一行，
--   而下面的 `command_idempotency_no_delete_guard` 拒绝一切 DELETE。
--
-- ---------------------------------------------------------------------------
-- 为什么「key 已超保留期」不在本表的状态机里
--
--   §4 第五行说「关键命令仍通过**唯一约束/业务引用**防重复；客户端不应复用」。
--   也就是说，保留期到了之后，**防线是本表的 UNIQUE 约束**，
--   不是一条可以算出「已过期」的状态。
--   把它建模成第五个状态会引入一个规格没有的过期判断，
--   而且会暗示「过期后可以放行」——与该行的意思正相反。
--   DATA_CONVENTIONS §6 的留存表也只说「关键命令至少 30 天」，
--   未冻结清理时机与批次，故本迁移不提供清理能力。
--
-- ---------------------------------------------------------------------------
-- 为什么 actor_fingerprint 是 32 字节 BLOB 而不是 TEXT
--
--   §4 只说键里有 actor_fingerprint，**没有规定它怎么算、什么形状**。
--   本迁移因此不规定算法的语义，只要求「同一 actor 恒得同一指纹」——
--   这正是幂等重放能工作的前提。选 32 字节是因为它容纳常见摘要长度，
--   且与 domain 层的 `ActorFingerprint` 一致；
--   **这不是安全声明**：指纹从不离开唯一键，不用于鉴权。
--
-- ---------------------------------------------------------------------------
-- 为什么 first_error_code 可空
--
--   state=IN_PROGRESS 时还没有结果，state=SUCCEEDED 时也没有错误码。
--   下面的 CHECK 保证「有错误码」与「state=FAILED」同时成立或同时不成立。
--   规格说「返回同一稳定失败」，所以失败码必须能被原样重放，
--   这要求它存得下来——但没规定码的词表，故用 TEXT 而非 CHECK 白名单。
--
-- ---------------------------------------------------------------------------
-- 为什么 payload_hash 是 32 字节而不是 TEXT
--
--   §4 只说「保存 payload hash」。选定宽 BLOB 是为了让
--   「同 payload」判断成为一次定长比较，且不依赖调用方提供规范化的十六进制串。
--   同样地：**算法由应用层注入，本迁移不规定**。
-- ===========================================================================

CREATE TABLE command_idempotency (
    team_id BLOB NOT NULL
        CHECK (
            typeof(team_id) = 'blob'
            AND length(team_id) = 16
            AND substr(hex(team_id), 13, 1) = '7'
            AND substr(hex(team_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    -- §4 的第二段。定宽、无安全声明，见上方说明。
    actor_fingerprint BLOB NOT NULL
        CHECK (
            typeof(actor_fingerprint) = 'blob'
            AND length(actor_fingerprint) = 32
        ),
    -- §4 的第三段。见 domain 层 CommandName::parse 的字符集约束；
    -- 这里只做非空与长度，字符集由应用层校验，避免两处定义漂移。
    command_name TEXT NOT NULL
        CHECK (length(trim(command_name)) > 0 AND length(command_name) <= 64),
    -- §4 的第四段，也是 DATA_CONVENTIONS §1 的 idempotency_key：
    -- 「调用者按逻辑操作生成的稳定字符串/UUID」。
    idempotency_key TEXT NOT NULL
        CHECK (length(trim(idempotency_key)) > 0 AND length(idempotency_key) <= 128),
    -- §4 的「payload hash」。算法由应用层注入，本表只比对。
    payload_hash BLOB NOT NULL
        CHECK (
            typeof(payload_hash) = 'blob'
            AND length(payload_hash) = 32
        ),
    -- §4 的「状态」。三个值对应 §4 表的前三行；
    -- 第四行是 UNIQUE 冲突，第五行不进入状态机（见上方说明）。
    state TEXT NOT NULL
        CHECK (state IN ('IN_PROGRESS', 'SUCCEEDED', 'FAILED')),
    -- §4 的「首次结果引用」。IN_PROGRESS 时必为空。
    result_ref TEXT
        CHECK (result_ref IS NULL OR (length(trim(result_ref)) > 0 AND length(result_ref) <= 128)),
    -- §4 的「返回同一稳定失败」需要把首次的码存下来才能原样重放。
    first_error_code TEXT
        CHECK (first_error_code IS NULL OR length(trim(first_error_code)) > 0),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    updated_at INTEGER NOT NULL CHECK (typeof(updated_at) = 'integer'),

    -- §4 的幂等记录键，四个部分缺一不可。
    CONSTRAINT command_idempotency_key_unique
        UNIQUE (team_id, actor_fingerprint, command_name, idempotency_key),
    CONSTRAINT command_idempotency_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

-- 状态与结果字段必须自洽，否则重放会拿到一个无法解释的记录：
--   IN_PROGRESS 不能带结果引用，也不能带错误码；
--   SUCCEEDED 必须带结果引用，且不带错误码；
--   FAILED     必须带错误码，且不带结果引用。
CREATE TRIGGER command_idempotency_state_consistency_insert
BEFORE INSERT ON command_idempotency
FOR EACH ROW
WHEN NOT (
    (NEW.state = 'IN_PROGRESS'
        AND NEW.result_ref IS NULL
        AND NEW.first_error_code IS NULL)
    OR (NEW.state = 'SUCCEEDED'
        AND NEW.result_ref IS NOT NULL
        AND NEW.first_error_code IS NULL)
    OR (NEW.state = 'FAILED'
        AND NEW.result_ref IS NULL
        AND NEW.first_error_code IS NOT NULL)
)
BEGIN
    SELECT RAISE(ABORT, 'command_idempotency: state must agree with result_ref and first_error_code');
END;

CREATE TRIGGER command_idempotency_state_consistency_update
BEFORE UPDATE ON command_idempotency
FOR EACH ROW
WHEN NOT (
    (NEW.state = 'IN_PROGRESS'
        AND NEW.result_ref IS NULL
        AND NEW.first_error_code IS NULL)
    OR (NEW.state = 'SUCCEEDED'
        AND NEW.result_ref IS NOT NULL
        AND NEW.first_error_code IS NULL)
    OR (NEW.state = 'FAILED'
        AND NEW.result_ref IS NULL
        AND NEW.first_error_code IS NOT NULL)
)
BEGIN
    SELECT RAISE(ABORT, 'command_idempotency: state must agree with result_ref and first_error_code');
END;

-- §4 第四行与第五行共同依赖「记录不能消失」：
-- 删掉一行会让同 key 的下一次调用被当成首次执行，
-- 于是同一个逻辑操作跑第二遍，Event/Run/Artifact 全部重复。
CREATE TRIGGER command_idempotency_no_delete_guard
BEFORE DELETE ON command_idempotency
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'command_idempotency records are append-only; a key past its retention window is still guarded by the unique constraint');
END;

-- ---------------------------------------------------------------------------
-- 为什么上面那个 DELETE 守卫**不够**，以及这个 replace_guard 为什么必须有
--
-- `no_delete_guard` 拦得住裸 `DELETE`，但拦不住 `INSERT OR REPLACE`。
-- SQLite 的 `INSERT OR REPLACE` 在唯一约束冲突时执行的是
-- **隐式 DELETE + INSERT**，而隐式 DELETE **不触发** DELETE 触发器
-- （除非显式 `PRAGMA recursive_triggers=ON`，本项目未设）。
--
-- 后果是可实测的：一次 `INSERT OR REPLACE` 同键不同值，会把
-- `payload_hash` 改写、把 `result_ref` 换成调用方给的值，
-- 于是 §4 第 2 行「返回首次结果」和第 4 行「不同 payload → 冲突」同时失效——
-- 首次结果被**原地劫持**。
--
-- 模式取自本项目已有的四处同款先例（`0013`/`0014`/`0016`
-- 的 `*_replace_guard`，其中 `0014` 的注释已写明
-- 「OR REPLACE 会先删除冲突行再插入，等于绕过上面全部 UPDATE 守卫」）。
-- `0022` 曾是唯一漏掉这个已知模式的表。
-- ---------------------------------------------------------------------------
CREATE TRIGGER command_idempotency_replace_guard
BEFORE INSERT ON command_idempotency
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM command_idempotency
    WHERE team_id = NEW.team_id
      AND actor_fingerprint = NEW.actor_fingerprint
      AND command_name = NEW.command_name
      AND idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'command_idempotency: a key is claimed by its first record; INSERT OR REPLACE must not hijack it');
END;

-- ---------------------------------------------------------------------------
-- 为什么 payload_hash 和 result_ref 需要 UPDATE 守卫（与上面是不同性质）
--
-- replace_guard 防的是「绕过守卫」，下面这两条防的是「**根本没有守卫**」：
-- 没有它们，同一 actor 拿到同 key 后，一条裸 UPDATE 就能改写
-- payload_hash（把「同 payload」判定翻转）或改写 result_ref（换掉首次结果）。
--
-- §4 规定「保存 payload hash」和「首次结果引用」，
-- 隐含它们是**首次尝试**的不可变事实：重放要比较的就是这��个值。
-- 记录的状态可以从 IN_PROGRESS 推进到终态，但**这四段永不改变**：
--   键四元组、payload_hash、创建时间。
-- ---------------------------------------------------------------------------
CREATE TRIGGER command_idempotency_identity_immutable_update
BEFORE UPDATE ON command_idempotency
FOR EACH ROW
WHEN NEW.team_id IS NOT OLD.team_id
  OR NEW.actor_fingerprint IS NOT OLD.actor_fingerprint
  OR NEW.command_name IS NOT OLD.command_name
  OR NEW.idempotency_key IS NOT OLD.idempotency_key
  OR NEW.payload_hash IS NOT OLD.payload_hash
  OR NEW.created_at IS NOT OLD.created_at
BEGIN
    SELECT RAISE(ABORT, 'command_idempotency: the §4 key and payload hash record the first attempt and are immutable');
END;

-- 首次结果引用同样不可改写：§4 第 2 行要「返回首次结果」，
-- 若允许原地替换，返回的就是第二次写入的值而不是首次的。
--
-- 「写一次」不等于「不能从空变为有值」——记录**必须**能从 IN_PROGRESS
-- 推进到终态，那一步就是首次写入 result_ref / first_error_code。
-- 所以这里拦的是**非空值被再次改写**，以及两个字段被同时设置；
-- 由空到非空的推进交给上面的 state_consistency 触发器校验。
CREATE TRIGGER command_idempotency_result_ref_immutable_update
BEFORE UPDATE ON command_idempotency
FOR EACH ROW
WHEN (OLD.result_ref IS NOT NULL AND NEW.result_ref IS NOT OLD.result_ref)
  OR (OLD.first_error_code IS NOT NULL
      AND NEW.first_error_code IS NOT OLD.first_error_code)
BEGIN
    SELECT RAISE(ABORT, 'command_idempotency: the first outcome is recorded once and replayed verbatim');
END;

-- 回放路径按 key 精确定位，走主键上的 UNIQUE 索引即可。
-- 另建一个 team 前缀索引，供 Team 归档/导出时按 Team 扫描
-- （DATA_CONVENTIONS §6：幂等记录随 Team 保留）。
CREATE INDEX command_idempotency_team_id ON command_idempotency (team_id);
