-- DEV-02-08 遗留修复：Message 与 Delivery Attempt 的 INSERT 侧状态机守卫。
--
-- 0017 建立了完整的 UPDATE 侧状态机：
--   messages_state_machine_update          (BEFORE UPDATE OF status)
--   delivery_attempts_state_machine_update  (BEFORE UPDATE OF status)
-- 两者都是 BEFORE UPDATE。0017 的对抗审查 35 条攻击也全部是 UPDATE 语义，
-- 因此整类「创建轴」绕过从未被尝试。
--
-- 实测复现（0001..0017 基线，10 条攻击全部绕过，2 条对照组正常）：
--   INSERT messages status='QUEUED'|'ROUTED'|'DELIVERED'|'ACKNOWLEDGED'
--                 |'REPLIED'|'EXPIRED'|'FAILED'|'NEEDS_HUMAN'   → 全部成功
--   INSERT delivery_attempts status='SUCCEEDED'|'ABANDONED'     → 全部成功
--   且 SUCCEEDED 的 attempt 其 completed_at 仍为 NULL。
--
-- 根因：状态机只约束「从什么状态迁到什么状态」，没有约束「创建时只能是初始态」。
-- messages_sender_binding_snapshot_insert 带 NEW.status != 'DRAFT' 条件，
-- 但它只用于校验快照是否存在，不构成状态机——一条携带合法快照的终态消息可以直接诞生。
--
-- 语义来源：docs/specs/STATE_MACHINES.md 第 5 节明文
--   「创建 → DRAFT」
--   「每次 attempt 单独记录 PENDING → IN_FLIGHT → SUCCEEDED/FAILED/ABANDONED」
-- 即 Message 只能以 DRAFT 诞生，DeliveryAttempt 只能以 PENDING 诞生。
-- 本迁移不新增任何状态、错误码或权限，只补上创建时的入口约束。
--
-- 为什么不需要升级守卫（与 0017 不同）：
-- 0017 的守卫约束的是「既有数据是否自相矛盾」，因此需要扫描存量并原子拒绝。
-- 本迁移约束的是「创建动作」，只对 INSERT 生效，不影响任何既有行——
-- 一条已经处于 DELIVERED 的历史消息完全合法，不应被拒绝。
--
-- 前向迁移：0001..0017 零修改，SQLx checksum 不漂移。

-- ---------------------------------------------------------------------------
-- Message 创建入口：只能是 DRAFT
-- ---------------------------------------------------------------------------
CREATE TRIGGER messages_state_machine_insert
BEFORE INSERT ON messages
FOR EACH ROW
WHEN NEW.status != 'DRAFT'
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_INVALID_TRANSITION');
END;

-- ---------------------------------------------------------------------------
-- DeliveryAttempt 创建入口：只能是 PENDING
-- ---------------------------------------------------------------------------
CREATE TRIGGER delivery_attempts_state_machine_insert
BEFORE INSERT ON delivery_attempts
FOR EACH ROW
WHEN NEW.status != 'PENDING'
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_INVALID_TRANSITION');
END;

-- ---------------------------------------------------------------------------
-- 终态时间戳不得在创建时出现。
-- 0017 的 UPDATE 侧状态机保证了迁移合法，但没有禁止「以终态时间戳配合非终态状态」
-- 诞生。status='PENDING' 的 attempt 却带 completed_at 会让「attempt 已完成」
-- 成为可查询的事实，与 status 自相矛盾；同理 status='DRAFT' 的 message 不应带
-- delivered_at / acknowledged_at / replied_at。
-- 状态集合取自 STATE_MACHINES.md 第 5 节；这里不判断时间戳的取值合理性，
-- 只拒绝「在尚未进入对应状态时就声称已发生」。
-- ---------------------------------------------------------------------------
CREATE TRIGGER delivery_attempts_initial_timestamps_insert
BEFORE INSERT ON delivery_attempts
FOR EACH ROW
WHEN NEW.completed_at IS NOT NULL
  OR NEW.started_at IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_PRECONDITION_FAILED');
END;

CREATE TRIGGER messages_initial_timestamps_insert
BEFORE INSERT ON messages
FOR EACH ROW
WHEN NEW.delivered_at IS NOT NULL
  OR NEW.acknowledged_at IS NOT NULL
  OR NEW.replied_at IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_PRECONDITION_FAILED');
END;

-- ---------------------------------------------------------------------------
-- 同一规则在 UPDATE 侧同样成立。
--
-- 第二遍对抗审查的 S-03：先合法插入 DRAFT，再
--   UPDATE messages SET status='QUEUED', ..., delivered_at=1000
-- 成功创建了一条「还没投递就已投递」的消息。上面的 INSERT 守卫管不到这里。
--
-- 语义来源：STATE_MACHINES.md 第 5 节的状态顺序
--   ROUTED -> DELIVERED -> ACKNOWLEDGED -> PROCESSING -> REPLIED
-- 决定了每个时间戳最早可能出现的状态。集合取自该迁移表，未新增状态。
-- ---------------------------------------------------------------------------
CREATE TRIGGER messages_timestamps_follow_state_update
BEFORE UPDATE OF delivered_at, acknowledged_at, replied_at, status ON messages
FOR EACH ROW
WHEN (NEW.delivered_at IS NOT NULL
      AND NEW.status NOT IN ('DELIVERED', 'ACKNOWLEDGED', 'PROCESSING', 'REPLIED'))
  OR (NEW.acknowledged_at IS NOT NULL
      AND NEW.status NOT IN ('ACKNOWLEDGED', 'PROCESSING', 'REPLIED'))
  OR (NEW.replied_at IS NOT NULL
      AND NEW.status NOT IN ('PROCESSING', 'REPLIED'))
  -- 已经发生过的时间戳不得被抹掉。
  OR (NEW.delivered_at IS NULL AND OLD.delivered_at IS NOT NULL)
  OR (NEW.acknowledged_at IS NULL AND OLD.acknowledged_at IS NOT NULL)
  OR (NEW.replied_at IS NULL AND OLD.replied_at IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_PRECONDITION_FAILED');
END;

-- ---------------------------------------------------------------------------
-- 离开 DRAFT 的那一刻必须已经冻结 sender Binding snapshot。
--
-- 0017 的 messages_sender_binding_snapshot_insert 在 INSERT 上校验
-- （NEW.sender_actor_type='ROLE' AND NEW.status != 'DRAFT'），
-- messages_sender_binding_locked_after_queued 在 UPDATE 上冻结
-- （OLD.status != 'DRAFT' 时不得改写快照）。
-- 两者之间有一道缝：**从 DRAFT 迁到 QUEUED 时快照可以完全不存在**。
-- 这道缝在 0018 之前不可见，因为那时的测试直接以 QUEUED 插入、
-- 恰好被 INSERT 守卫挡住；强制以 DRAFT 诞生之后，这条路径才暴露。
--
-- 语义来源：STATE_MACHINES.md 第 5 节把「冻结内容，记录 sender Binding
-- snapshot」定为 DRAFT -> QUEUED 的副作用。快照是该迁移的产物，
-- 不是可选项。
-- ---------------------------------------------------------------------------
CREATE TRIGGER messages_sender_binding_snapshot_on_queuing
BEFORE UPDATE OF status ON messages
FOR EACH ROW
WHEN OLD.status = 'DRAFT'
 AND NEW.status = 'QUEUED'
 AND NEW.sender_actor_type = 'ROLE'
 AND (
     NEW.sender_binding_snapshot_id IS NULL
     OR NEW.sender_binding_generation IS NULL
     OR NOT EXISTS (
         SELECT 1
         FROM bindings
         WHERE id = NEW.sender_binding_snapshot_id
           AND team_id = NEW.team_id
           AND role_id = NEW.from_role_id
           AND binding_generation = NEW.sender_binding_generation
     )
 )
BEGIN
    SELECT RAISE(ABORT, 'Message sender Binding snapshot violation');
END;
