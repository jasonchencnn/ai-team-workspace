-- DEV-02-09：Notification、Policy 与 AttentionItem 的完整性与状态机守卫。
--
-- 0008 定义了三张表（176 行、7 个索引）但**自身零 trigger**。0014 只为
-- notifications 装了三件防护，且都带
--   WHEN EXISTS (... events WHERE subject_type='NOTIFICATION' AND subject_id=...)
-- 条件，即**只对被 Event 引用的行生效**；attention_items 在 0014 中零 trigger。
--
-- 实现前的四轴对抗审查（创建/变更/删除/替换）在 0001..0018 基线上实测复现
-- 28 项绕过：通知与待办可以生而终态、可以复活、可以跨 Team 寻址、
-- 内容可改写、时间戳与状态脱节、可被 OR REPLACE 销毁。
--
-- 全部状态集合逐条取自 docs/specs/STATE_MACHINES.md 第 6 节，未增删状态、
-- 错误码或权限。吸收态判定依据：该节迁移表中没有任何一行以 DISMISSED、
-- EXPIRED（Notification）或 RESOLVED、DISMISSED（AttentionItem）为 From。
--
-- 前向迁移：0001..0018 与 0008 原文件零修改，SQLx checksum 不漂移。

-- ===========================================================================
-- 升级守卫：三类脏历史原子拒绝
-- ===========================================================================
CREATE TABLE notification_attention_integrity_upgrade_guard (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    valid INTEGER NOT NULL CHECK (valid = 1)
);

INSERT INTO notification_attention_integrity_upgrade_guard (id, valid)
SELECT 1, 0
WHERE EXISTS (
    -- 通知的源 Event 属于别的 Team。
    SELECT 1
    FROM notifications AS n
    JOIN events AS e ON e.id = n.source_event_id
    WHERE e.team_id IS NOT n.team_id
)
OR EXISTS (
    -- 通知的源 Message 属于别的 Team。
    SELECT 1
    FROM notifications AS n
    JOIN messages AS m ON m.id = n.source_message_id
    WHERE m.team_id IS NOT n.team_id
)
OR EXISTS (
    -- 通知引用了别的 Team 的 Policy。
    SELECT 1
    FROM notifications AS n
    JOIN notification_policies AS p ON p.id = n.policy_id
    WHERE p.team_id IS NOT n.team_id
)
OR EXISTS (
    -- Policy 挂在别的 Team 上。
    SELECT 1
    FROM notification_policies AS p
    JOIN teams AS t ON t.id = p.team_id
    WHERE t.id IS NULL
)
OR EXISTS (
    -- Attention 的 source_id 指向不存在的 Task。
    SELECT 1
    FROM attention_items AS a
    WHERE a.source_type = 'TASK'
      AND NOT EXISTS (SELECT 1 FROM tasks AS t WHERE t.id = a.source_id)
)
OR EXISTS (
    -- Attention 的 source_id 指向不存在的 Run。
    SELECT 1
    FROM attention_items AS a
    WHERE a.source_type = 'RUN'
      AND NOT EXISTS (SELECT 1 FROM runs AS r WHERE r.id = a.source_id)
)
OR EXISTS (
    -- Attention 的 source_id 指向不存在的 Message。
    SELECT 1
    FROM attention_items AS a
    WHERE a.source_type = 'MESSAGE'
      AND NOT EXISTS (SELECT 1 FROM messages AS m WHERE m.id = a.source_id)
)
OR EXISTS (
    -- Attention 的 source_id 指向不存在的 Approval。
    SELECT 1
    FROM attention_items AS a
    WHERE a.source_type = 'APPROVAL'
      AND NOT EXISTS (SELECT 1 FROM approvals AS ap WHERE ap.id = a.source_id)
);

-- ===========================================================================
-- Notification：创建入口
-- 第 6 节迁移表首行为「创建 → UNREAD」，故通知只能以 UNREAD 诞生。
-- ===========================================================================
CREATE TRIGGER notifications_state_machine_insert
BEFORE INSERT ON notifications
FOR EACH ROW
WHEN NEW.status != 'UNREAD'
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_INVALID_TRANSITION');
END;

CREATE TRIGGER notifications_initial_timestamps_insert
BEFORE INSERT ON notifications
FOR EACH ROW
WHEN NEW.read_at IS NOT NULL OR NEW.dismissed_at IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_PRECONDITION_FAILED');
END;

-- ===========================================================================
-- Notification：状态机
--   UNREAD -> READ | DISMISSED | EXPIRED
--   READ   -> DISMISSED | EXPIRED
--   DISMISSED 与 EXPIRED 均无出边（第 6 节迁移表无以二者为 From 的行）。
-- ===========================================================================
CREATE TRIGGER notifications_state_machine_update
BEFORE UPDATE OF status ON notifications
FOR EACH ROW
WHEN NOT (
       (OLD.status = 'UNREAD' AND NEW.status IN ('READ', 'DISMISSED', 'EXPIRED'))
    OR (OLD.status = 'READ'   AND NEW.status IN ('DISMISSED', 'EXPIRED'))
)
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_INVALID_TRANSITION');
END;

-- 时间戳只禁止「提前声称」，不强制「必须写入」。
-- 第 6 节没有规定 read_at / dismissed_at 必须在迁移的同一次写入中落库，
-- 强制写入等于发明语义；真正的自相矛盾是尚未进入对应状态就声称已发生。
-- 一条 READ 之后再 DISMISSED 的通知会同时带着 read_at 与 dismissed_at，
-- 这完全合法，因此只在「状态尚未到达」时拒绝。
CREATE TRIGGER notifications_timestamp_consistency_update
BEFORE UPDATE ON notifications
FOR EACH ROW
WHEN (NEW.status = 'UNREAD' AND NEW.read_at IS NOT NULL)
  OR (NEW.status != 'DISMISSED' AND NEW.dismissed_at IS NOT NULL)
  -- 第二遍对抗审查的 S-09：UNREAD -> READ 写入 read_at 后，
  -- 再把 read_at 抹成 NULL 即可弱化「已读」这一审计事实。
  -- 一旦发生过的读/撤时间戳不得被抹除。
  OR (NEW.read_at IS NULL AND OLD.read_at IS NOT NULL)
  OR (NEW.dismissed_at IS NULL AND OLD.dismissed_at IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_PRECONDITION_FAILED');
END;

-- ===========================================================================
-- Notification：同一 Team 寻址
-- 节点标准要求「Event、Message、通知和待处理项可互相引用」，跨 Team 引用
-- 等于把一个 Team 的事实投递到另一个 Team 的收件人。
-- ===========================================================================
CREATE TRIGGER notifications_team_consistency_insert
BEFORE INSERT ON notifications
FOR EACH ROW
WHEN (NEW.source_event_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM events AS e WHERE e.id = NEW.source_event_id AND e.team_id = NEW.team_id))
  OR (NEW.source_message_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM messages AS m WHERE m.id = NEW.source_message_id AND m.team_id = NEW.team_id))
  OR (NEW.policy_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM notification_policies AS p WHERE p.id = NEW.policy_id AND p.team_id = NEW.team_id))
BEGIN
    SELECT RAISE(ABORT, 'Notification Team consistency violation');
END;

CREATE TRIGGER notifications_team_consistency_update
BEFORE UPDATE OF team_id, source_event_id, source_message_id, policy_id ON notifications
FOR EACH ROW
WHEN (NEW.source_event_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM events AS e WHERE e.id = NEW.source_event_id AND e.team_id = NEW.team_id))
  OR (NEW.source_message_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM messages AS m WHERE m.id = NEW.source_message_id AND m.team_id = NEW.team_id))
  OR (NEW.policy_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM notification_policies AS p WHERE p.id = NEW.policy_id AND p.team_id = NEW.team_id))
  OR NEW.team_id IS NOT OLD.team_id
BEGIN
    SELECT RAISE(ABORT, 'Notification Team consistency violation');
END;

-- ===========================================================================
-- Notification：投递呈现可读，但事实来源仍是 Event
-- DOMAIN_MODEL.md 把 Notification 定为「可读/确认，事实来源仍是 Event」，
-- DATA_CONVENTIONS.md 规定 created_at 不修改。因此收件人、去重键、严重级、
-- 标题、正文、来源与创建时间在创建后不可改写；否则「已发出的通知」可以被
-- 事后改写成另一条通知，而 Event 侧仍记录着原文。
-- ===========================================================================
CREATE TRIGGER notifications_content_immutable_update
BEFORE UPDATE OF recipient_human_id, dedupe_key, severity, title, body,
                      source_event_id, source_message_id, policy_id,
                      created_at, team_id, id
ON notifications
FOR EACH ROW
WHEN NEW.recipient_human_id IS NOT OLD.recipient_human_id
  OR NEW.dedupe_key         IS NOT OLD.dedupe_key
  OR NEW.severity           IS NOT OLD.severity
  OR NEW.title              IS NOT OLD.title
  OR NEW.body               IS NOT OLD.body
  OR NEW.created_at         IS NOT OLD.created_at
  OR NEW.id                 IS NOT OLD.id
BEGIN
    SELECT RAISE(ABORT, 'Notification content is immutable');
END;

-- version 只增不减（DATA_CONVENTIONS.md）。
CREATE TRIGGER notifications_version_monotonic_update
BEFORE UPDATE OF version ON notifications
FOR EACH ROW
WHEN NEW.version < OLD.version
BEGIN
    SELECT RAISE(ABORT, 'CONCURRENCY_VERSION_CONFLICT');
END;

-- OR REPLACE 会先删除冲突行再插入，等于绕过上面全部 UPDATE 守卫。
CREATE TRIGGER notifications_replace_guard
BEFORE INSERT ON notifications
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM notifications AS existing WHERE existing.id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'Notification identity is immutable');
END;

-- ===========================================================================
-- AttentionItem：创建入口
-- 第 6 节迁移表以「Attention OPEN -> ...」为起点，故待办只能以 OPEN 诞生。
-- ===========================================================================
CREATE TRIGGER attention_items_state_machine_insert
BEFORE INSERT ON attention_items
FOR EACH ROW
WHEN NEW.status != 'OPEN'
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_INVALID_TRANSITION');
END;

CREATE TRIGGER attention_items_initial_timestamps_insert
BEFORE INSERT ON attention_items
FOR EACH ROW
WHEN NEW.resolved_at IS NOT NULL OR NEW.dismissed_at IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_PRECONDITION_FAILED');
END;

-- ===========================================================================
-- AttentionItem：状态机
--   OPEN -> RESOLVED | DISMISSED，二者均无出边。
--   RESOLVED 必须带 resolved_at 且不得带 dismissed_at/dismiss_reason；
--   DISMISSED 必须带 dismissed_at 与 dismiss_reason（第 6 节「提供原因」），
--   且二者互斥——0008 的 CHECK 只约束终态方向，不约束两个终态不能并存。
-- ===========================================================================
CREATE TRIGGER attention_items_state_machine_update
BEFORE UPDATE OF status ON attention_items
FOR EACH ROW
WHEN NOT (OLD.status = 'OPEN' AND NEW.status IN ('RESOLVED', 'DISMISSED'))
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_INVALID_TRANSITION');
END;

-- 同样只禁止「提前声称」与「终态并存」，不强制写入时间戳。
-- RESOLVED 与 DISMISSED 是两条互斥的终态：0008 的 CHECK 只约束终态方向，
-- 不约束两个终态不能同时出现在一行上。
CREATE TRIGGER attention_items_timestamp_consistency_update
BEFORE UPDATE ON attention_items
FOR EACH ROW
WHEN (NEW.status = 'OPEN'      AND (NEW.resolved_at IS NOT NULL OR NEW.dismissed_at IS NOT NULL
                                    OR NEW.dismiss_reason IS NOT NULL))
  OR (NEW.status = 'RESOLVED'  AND (NEW.dismissed_at IS NOT NULL OR NEW.dismiss_reason IS NOT NULL))
  OR (NEW.status = 'DISMISSED' AND NEW.resolved_at IS NOT NULL)
  OR (NEW.status = 'DISMISSED' AND NEW.dismiss_reason IS NULL)
  OR (NEW.status != 'DISMISSED' AND NEW.dismiss_reason IS NOT NULL)
  OR (NEW.status != 'RESOLVED' AND NEW.status != 'DISMISSED' AND NEW.resolved_at IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_PRECONDITION_FAILED');
END;

-- ===========================================================================
-- AttentionItem：多态来源寻址
--
-- source_id 是裸 BLOB，0008 无 FK（source_type 有五种取值，无法建单一 FK）。
-- 本迁移按 source_type 分派到后继表，校验存在性与同 Team。
--
-- source_type = 'SYSTEM' 没有后继表，其 source_id 无法校验——这是规格缺口
-- （第 6 节只点名 Approval/Run/Message，TASK 与 SYSTEM 的语义未定义），
-- 登记为遗留风险，不在本节点发明映射。
-- ===========================================================================
CREATE TRIGGER attention_items_source_consistency_insert
BEFORE INSERT ON attention_items
FOR EACH ROW
WHEN (NEW.source_type = 'TASK'    AND NOT EXISTS (
          SELECT 1 FROM tasks AS t WHERE t.id = NEW.source_id AND t.team_id = NEW.team_id))
  OR (NEW.source_type = 'RUN'     AND NOT EXISTS (
          SELECT 1 FROM runs AS r WHERE r.id = NEW.source_id AND r.team_id = NEW.team_id))
  OR (NEW.source_type = 'MESSAGE' AND NOT EXISTS (
          SELECT 1 FROM messages AS m WHERE m.id = NEW.source_id AND m.team_id = NEW.team_id))
  OR (NEW.source_type = 'APPROVAL' AND NOT EXISTS (
          SELECT 1 FROM approvals AS ap WHERE ap.id = NEW.source_id AND ap.team_id = NEW.team_id))
BEGIN
    SELECT RAISE(ABORT, 'AttentionItem source consistency violation');
END;

CREATE TRIGGER attention_items_source_consistency_update
BEFORE UPDATE OF team_id, source_type, source_id ON attention_items
FOR EACH ROW
WHEN NEW.team_id IS NOT OLD.team_id
  OR (NEW.source_type = 'TASK'    AND NOT EXISTS (
          SELECT 1 FROM tasks AS t WHERE t.id = NEW.source_id AND t.team_id = NEW.team_id))
  OR (NEW.source_type = 'RUN'     AND NOT EXISTS (
          SELECT 1 FROM runs AS r WHERE r.id = NEW.source_id AND r.team_id = NEW.team_id))
  OR (NEW.source_type = 'MESSAGE' AND NOT EXISTS (
          SELECT 1 FROM messages AS m WHERE m.id = NEW.source_id AND m.team_id = NEW.team_id))
  OR (NEW.source_type = 'APPROVAL' AND NOT EXISTS (
          SELECT 1 FROM approvals AS ap WHERE ap.id = NEW.source_id AND ap.team_id = NEW.team_id))
BEGIN
    SELECT RAISE(ABORT, 'AttentionItem source consistency violation');
END;

-- 待办的标题、摘要、类型与创建时间是「待处理索引」的快照，
-- 底层事项变了应产生新待办，而不是改写既有待办。
CREATE TRIGGER attention_items_content_immutable_update
BEFORE UPDATE OF attention_type, source_type, title, summary, created_at, id, team_id
ON attention_items
FOR EACH ROW
WHEN NEW.attention_type IS NOT OLD.attention_type
  OR NEW.title         IS NOT OLD.title
  OR NEW.summary       IS NOT OLD.summary
  OR NEW.created_at    IS NOT OLD.created_at
  OR NEW.id            IS NOT OLD.id
BEGIN
    SELECT RAISE(ABORT, 'AttentionItem content is immutable');
END;

CREATE TRIGGER attention_items_version_monotonic_update
BEFORE UPDATE OF version ON attention_items
FOR EACH ROW
WHEN NEW.version < OLD.version
BEGIN
    SELECT RAISE(ABORT, 'CONCURRENCY_VERSION_CONFLICT');
END;

CREATE TRIGGER attention_items_replace_guard
BEFORE INSERT ON attention_items
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM attention_items AS existing WHERE existing.id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'AttentionItem identity is immutable');
END;

-- ===========================================================================
-- NotificationPolicy：version 只增不减，且不得跨 Team 移动
-- DOMAIN_MODEL.md 规定 Policy「版本化」。
-- ===========================================================================
CREATE TRIGGER notification_policies_version_monotonic_update
BEFORE UPDATE OF version ON notification_policies
FOR EACH ROW
WHEN NEW.version < OLD.version
BEGIN
    SELECT RAISE(ABORT, 'CONCURRENCY_VERSION_CONFLICT');
END;

CREATE TRIGGER notification_policies_team_fixed_update
BEFORE UPDATE OF team_id, id ON notification_policies
FOR EACH ROW
WHEN NEW.team_id IS NOT OLD.team_id OR NEW.id IS NOT OLD.id
BEGIN
    SELECT RAISE(ABORT, 'NotificationPolicy identity is immutable');
END;

CREATE TRIGGER notification_policies_replace_guard
BEFORE INSERT ON notification_policies
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM notification_policies AS existing WHERE existing.id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'NotificationPolicy identity is immutable');
END;
