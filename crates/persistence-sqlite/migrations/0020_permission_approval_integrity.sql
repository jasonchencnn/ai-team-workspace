-- DEV-02-10：Permission 策略与 Approval 的 Team 一致性、状态机与不可变性守卫。
--
-- 0009 定义了两张表（137 行、3 个索引、29 个 CHECK、0 个 trigger）。
-- 它自身的三条表级 CHECK 是有意义的（禁自批、决定需 Human、拒绝需理由），
-- 但缺少 trigger 级的 Team 一致性、状态机、创建入口、不可变性与替换/删除守卫。
--
-- 实现前的四轴对抗审查（创建/变更/删除/替换）在 0001..0019 基线上实测复现
-- 24 项绕过：Approval 可生而终态、可复活、可改写授权范围、可跨 Team 寻址；
-- permission 策略可被改写与 OR REPLACE 放大 scope。
--
-- 全部状态集合逐条取自 docs/specs/STATE_MACHINES.md 第 7 节：
--   创建 -> PENDING
--   PENDING -> APPROVED | REJECTED | EXPIRED | CANCELLED
-- 「全部终态不可重开；新请求必须新建 Approval」——第 7 节第 175 行原文。
--
-- permissions 表**没有状态机**：该表无 status 列，冻结规格中也不存在
-- Permission 状态（STATE_MACHINES 第 2~7 节覆盖 Task/Run/Binding/Message/
-- Notification/Attention/Approval，无 Permission）。故对 permissions 只做
-- Team 一致性、version 单调与身份/替换守卫，不发明状态集合。
--
-- 前向迁移：0001..0019 与 0009 原文件零修改，SQLx checksum 不漂移。

-- ===========================================================================
-- 升级守卫：跨 Team 寻址原子拒绝
-- ===========================================================================
CREATE TABLE permission_approval_integrity_upgrade_guard (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    valid INTEGER NOT NULL CHECK (valid = 1)
);

INSERT INTO permission_approval_integrity_upgrade_guard (id, valid)
SELECT 1, 0
WHERE EXISTS (
    -- Approval 的请求者 Role 属于别的 Team。
    SELECT 1
    FROM approvals AS a
    JOIN roles AS r ON r.id = a.requester_role_id
    WHERE r.team_id IS NOT a.team_id
)
OR EXISTS (
    -- permission 策略挂在别的 Role 上。
    SELECT 1
    FROM permissions AS p
    JOIN roles AS r ON r.id = p.role_id
    WHERE r.team_id IS NOT p.team_id
)
OR EXISTS (
    -- requester_actor_type=ROLE 却没有 Role 归属。
    SELECT 1
    FROM approvals AS a
    WHERE a.requester_actor_type = 'ROLE' AND a.requester_role_id IS NULL
)
OR EXISTS (
    -- requester_actor_type=ROLE 却同时带 Human 身份：一条审批只能有一个请求者。
    SELECT 1
    FROM approvals AS a
    WHERE a.requester_actor_type = 'ROLE' AND a.requester_human_id IS NOT NULL
)
OR EXISTS (
    -- requester_actor_type=HUMAN/SYSTEM 却带 Role 归属。
    SELECT 1
    FROM approvals AS a
    WHERE a.requester_actor_type IN ('HUMAN', 'SYSTEM') AND a.requester_role_id IS NOT NULL
);

-- ===========================================================================
-- Approval：创建入口
-- 第 7 节迁移表首行为「创建 -> PENDING」，故审批只能以 PENDING 诞生，
-- 且创建时不得携带任何决定痕迹。
-- ===========================================================================
CREATE TRIGGER approvals_state_machine_insert
BEFORE INSERT ON approvals
FOR EACH ROW
WHEN NEW.status != 'PENDING'
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_INVALID_TRANSITION');
END;

-- 决定痕迹：创建时不得「提前声称」。
--
--   依据：第 7 节首行为「创建 -> PENDING」，而 PENDING 尚未发生决定，
--   decided_by_human_id / decided_at 此刻必为空。
--
-- **不拦 decision_notes**（这一条是第二遍对抗审查的 TC-01 推翻的）：
--   我原先在这里写了 `OR NEW.decision_notes IS NOT NULL`，理由是
--   「决定说明属于决定，未决定时不该有」。但：
--     1. 规格无此明文 —— 第 7 节只对 REJECTED 要求「提供决定/可选原因」，
--        对 PENDING 携带 notes 未置一词；
--     2. 该条件造成守卫自相矛盾 —— approvals_decision_consistency_update
--        只管 decided_at / decided_by_human_id，不管 notes，于是
--        「创建时带 notes」被拦、「事后 UPDATE 补 notes」却被放行。
--        一个前后不一致的守卫比没有守卫更危险：它让人以为该面已封，
--        实际留了后门。
--     3. 该条件不防任何已论证的攻击 —— decision_notes 不参与自批 CHECK、
--        不参与 grant 计算、不参与决定人追溯；status 才是权威字段，
--        PENDING 带 notes 不会被误读为已批准。
--   故按「不发明语义」原则移除。若将来规格明文要求 PENDING 不得携带
--   决定说明，需**同时**加在 INSERT 与 UPDATE 两侧，不可只加一侧。
CREATE TRIGGER approvals_initial_decision_insert
BEFORE INSERT ON approvals
FOR EACH ROW
WHEN NEW.decided_by_human_id IS NOT NULL
  OR NEW.decided_at IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_PRECONDITION_FAILED');
END;

-- 请求者身份必须与 actor_type 自洽：ROLE 必须有 Role 归属且不得带 Human id；
-- HUMAN/SYSTEM 不得带 Role 归属。
-- 注意：Human id 本身的合法性不在此校验——全库无 humans 表，
-- requester_human_id 与 roles.id 都是 16 字节 UUIDv7 BLOB，DB 层分不出。
-- 该项为 Human 已裁决的显式降级，登记在终审文档。
CREATE TRIGGER approvals_requester_identity_insert
BEFORE INSERT ON approvals
FOR EACH ROW
WHEN (NEW.requester_actor_type = 'ROLE' AND NEW.requester_role_id IS NULL)
  OR (NEW.requester_actor_type = 'ROLE' AND NEW.requester_human_id IS NOT NULL)
  OR (NEW.requester_actor_type IN ('HUMAN', 'SYSTEM') AND NEW.requester_role_id IS NOT NULL)
  OR (NEW.requester_role_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM roles AS r WHERE r.id = NEW.requester_role_id AND r.team_id = NEW.team_id))
BEGIN
    SELECT RAISE(ABORT, 'Approval requester identity violation');
END;

-- ===========================================================================
-- Approval：状态机
--   PENDING -> APPROVED | REJECTED | EXPIRED | CANCELLED
-- 四个终态均无出边（第 7 节：全部终态不可重开）。
-- ===========================================================================
CREATE TRIGGER approvals_state_machine_update
BEFORE UPDATE OF status ON approvals
FOR EACH ROW
WHEN NOT (OLD.status = 'PENDING'
          AND NEW.status IN ('APPROVED', 'REJECTED', 'EXPIRED', 'CANCELLED'))
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_INVALID_TRANSITION');
END;

-- 决定痕迹：禁止「提前声称」、禁止「事后抹除」，并禁止「事后改值」。
--
--   PENDING 不得携带任何决定痕迹（还没决定就说决定了）；
--   一旦写下 decided_at / decided_by_human_id 不得抹成 NULL；
--   终态一旦落定，决定人与决定时刻**双向**冻结（既不许换成别人，也不许回填）。
--
-- 为什么终态要双向冻结（这是第二遍对抗审查的 T-01 / T-02）：
--   第 7 节把「decider 与 requester 不构成自批」列为 PENDING -> APPROVED/REJECTED
--   的**前置条件**，而 0009 的 CHECK（decided_by_human_id != requester_human_id）
--   正是靠 decided_by_human_id 的**值**来执行这条禁令。
--   一旦决定人可以事后改写，那条 CHECK 立刻失效：一条真实的
--   「A 请求 / B 批准」可以被改成「A 请求 / C 批准」—— CHECK 照样通过，
--   但 B 的批准痕迹被彻底抹掉，等于伪造一个从未发生过的批准。
--   决定人是该次迁移的**执行事实**，不是可事后订正的备注。
--
--   双向而不是单向：EXPIRED 由 Scheduler 在 expires_at 到达时发生（7 节），
--   **不需要任何人决定**；事后往 EXPIRED/CANCELLED 行回填一个「谁决定的」，
--   同样是往审计记录里加一个没发生过的痕迹。
--
-- 为什么不对终态强制写入（仍然只禁止、不要求）：
--   APPROVED/REJECTED 已有 0009 的 approvals_decision_requires_human 覆盖
--   （必须有决定人且必须有 decided_at），这是规格明文依据；
--   EXPIRED 的时刻已由 expires_at 记录，第 7 节未要求另记决定时间；
--   CANCELLED 的 actor 按第 7 节是「requester/Human」，requester 可能是 Role，
--   而表中只有 decided_by_human_id 一个决定人列，没有 role 决定人列——
--   强制 CANCELLED 携带决定人等于发明一个不存在的语义。
--   「记录取消 actor」在当前 schema 下**无落点**，登记为规格缺口。
--
-- 错误码沿用本文件既有的 DOMAIN_PRECONDITION_FAILED，不新增错误码。
CREATE TRIGGER approvals_decision_consistency_update
BEFORE UPDATE ON approvals
FOR EACH ROW
WHEN (NEW.status = 'PENDING' AND (NEW.decided_at IS NOT NULL
                                  OR NEW.decided_by_human_id IS NOT NULL))
  OR (NEW.decided_at IS NULL AND OLD.decided_at IS NOT NULL)
  OR (NEW.decided_by_human_id IS NULL AND OLD.decided_by_human_id IS NOT NULL)
  OR (OLD.status IN ('APPROVED', 'REJECTED', 'EXPIRED', 'CANCELLED')
      AND (NEW.decided_at IS NOT OLD.decided_at
           OR NEW.decided_by_human_id IS NOT OLD.decided_by_human_id))
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_PRECONDITION_FAILED');
END;

-- ===========================================================================
-- Approval：请求内容不可变
--
-- 第 7 节把 action、resource scope、理由、期限列为「创建 -> PENDING」的**前置条件**，
-- 而非可后续调整的字段。一条审批一旦发出，它请求什么就不是请求者能再改的；
-- 需要不同范围必须新建一条审批（「新请求必须新建 Approval」）。
-- expires_at 同样不可延长：期限是审批的一部分，不是可以商量的参数。
-- ===========================================================================
CREATE TRIGGER approvals_request_immutable_update
BEFORE UPDATE OF requester_actor_type, requester_role_id, requester_human_id,
                      action, resource_type, resource_scope, reason,
                      expires_at, created_at, team_id, id
ON approvals
FOR EACH ROW
WHEN NEW.requester_actor_type IS NOT OLD.requester_actor_type
  OR NEW.requester_role_id      IS NOT OLD.requester_role_id
  OR NEW.requester_human_id     IS NOT OLD.requester_human_id
  OR NEW.action                IS NOT OLD.action
  OR NEW.resource_type         IS NOT OLD.resource_type
  OR NEW.resource_scope        IS NOT OLD.resource_scope
  OR NEW.reason                IS NOT OLD.reason
  OR NEW.expires_at            IS NOT OLD.expires_at
  OR NEW.created_at            IS NOT OLD.created_at
  OR NEW.team_id               IS NOT OLD.team_id
  OR NEW.id                    IS NOT OLD.id
BEGIN
    SELECT RAISE(ABORT, 'Approval request is immutable');
END;

-- 请求者与 Team 的一致性在更新路径上同样成立。
CREATE TRIGGER approvals_team_consistency_update
BEFORE UPDATE OF team_id, requester_role_id ON approvals
FOR EACH ROW
WHEN NEW.team_id IS NOT OLD.team_id
  OR (NEW.requester_role_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM roles AS r WHERE r.id = NEW.requester_role_id AND r.team_id = NEW.team_id))
  OR (NEW.requester_actor_type = 'ROLE' AND NEW.requester_human_id IS NOT NULL)
  OR (NEW.requester_actor_type IN ('HUMAN', 'SYSTEM') AND NEW.requester_role_id IS NOT NULL)
  OR (NEW.requester_actor_type = 'ROLE' AND NEW.requester_role_id IS NULL)
BEGIN
    SELECT RAISE(ABORT, 'Approval requester identity violation');
END;

CREATE TRIGGER approvals_version_monotonic_update
BEFORE UPDATE OF version ON approvals
FOR EACH ROW
WHEN NEW.version < OLD.version
BEGIN
    SELECT RAISE(ABORT, 'CONCURRENCY_VERSION_CONFLICT');
END;

-- OR REPLACE 会先删冲突行再插入，等于绕过上面全部 UPDATE 守卫，
-- 并把一条已决定的审批换回 PENDING。
CREATE TRIGGER approvals_replace_guard
BEFORE INSERT ON approvals
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM approvals AS existing WHERE existing.id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'Approval identity is immutable');
END;

-- 已归档的审批是授权审计证据，第 7 节规定「新请求必须新建 Approval」——
-- 即终态记录不得删除，只能被新记录取代。
CREATE TRIGGER approvals_terminal_delete_guard
BEFORE DELETE ON approvals
FOR EACH ROW
WHEN OLD.status != 'PENDING'
BEGIN
    SELECT RAISE(ABORT, 'Approval decision history is immutable');
END;

-- ===========================================================================
-- Permission 策略：Team 一致性与 version
--
-- 该表无状态机（见文件头说明），只做三件事。
-- role_id 为空表示 Team 级策略，属正常形态，不要求其非空。
-- ===========================================================================
CREATE TRIGGER permissions_team_consistency_insert
BEFORE INSERT ON permissions
FOR EACH ROW
WHEN NEW.role_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM roles AS r WHERE r.id = NEW.role_id AND r.team_id = NEW.team_id)
BEGIN
    SELECT RAISE(ABORT, 'Permission Team consistency violation');
END;

CREATE TRIGGER permissions_team_consistency_update
BEFORE UPDATE OF team_id, role_id ON permissions
FOR EACH ROW
WHEN NEW.team_id IS NOT OLD.team_id
  OR (NEW.role_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM roles AS r WHERE r.id = NEW.role_id AND r.team_id = NEW.team_id))
BEGIN
    SELECT RAISE(ABORT, 'Permission Team consistency violation');
END;

-- DOMAIN_MODEL.md 规定 Permission Policy「版本化」，version 只增不减。
CREATE TRIGGER permissions_version_monotonic_update
BEFORE UPDATE OF version ON permissions
FOR EACH ROW
WHEN NEW.version < OLD.version
BEGIN
    SELECT RAISE(ABORT, 'CONCURRENCY_VERSION_CONFLICT');
END;

-- 改策略本身是允许的（PERMISSION_MATRIX.md 把 NotificationPolicy 的修改
-- 授权给 Leader/Owner），因此这里**不做内容不可变**——那会剥夺规格明确赋予
-- 的配置权。但「版本化」意味着每次策略变更都必须留下版本痕迹：
-- 改了 effect / scope_pattern / action / resource_type / is_human_only / role_id
-- 就必须同步递增 version，否则一次权限扩大可以不留任何审计痕迹。
-- 0009 的 CHECK 只保证 version >= 1，拦不住「改了内容但 version 不动」。
CREATE TRIGGER permissions_change_requires_version_bump
BEFORE UPDATE ON permissions
FOR EACH ROW
WHEN (NEW.effect        IS NOT OLD.effect
   OR NEW.scope_pattern IS NOT OLD.scope_pattern
   OR NEW.action        IS NOT OLD.action
   OR NEW.resource_type IS NOT OLD.resource_type
   OR NEW.is_human_only IS NOT OLD.is_human_only
   OR NEW.role_id       IS NOT OLD.role_id)
  AND NEW.version <= OLD.version
BEGIN
    SELECT RAISE(ABORT, 'CONCURRENCY_VERSION_CONFLICT');
END;

-- 策略身份不可变：OR REPLACE 不得销毁已生效的策略再以放大 scope 的版本重建。
CREATE TRIGGER permissions_replace_guard
BEFORE INSERT ON permissions
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM permissions AS existing WHERE existing.id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'Permission identity is immutable');
END;

-- 策略是权限求交的输入（DEV-02-13），删除会让既有的授权判定失去依据。
CREATE TRIGGER permissions_delete_guard
BEFORE DELETE ON permissions
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Permission policy is immutable');
END;
