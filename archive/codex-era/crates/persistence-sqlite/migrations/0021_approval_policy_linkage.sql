-- ===========================================================================
-- 0021_approval_policy_linkage.sql
--
-- 补齐 DEV-02-10 登记的两项规格缺口（Human 于 2026-09-29 裁决「按倾向解决」）：
--
--   A1  approvals.permission_id —— 让一条审批能指明它覆盖哪条权限策略。
--   B2  approvals.cancelled_by_actor_type / _actor_id —— 给 §7 要求的
--       「记录取消 actor」一个落点。
--
-- 本迁移**只做 ADD COLUMN**，不改写 0001～0020 任何一行。
--
-- ---------------------------------------------------------------------------
-- 为什么没有加 FOREIGN KEY
--
--   SQLite 的 `ALTER TABLE ADD COLUMN` **不允许**列上带 `REFERENCES` 子句。
--   要真正的外键只能重建整张表，代价与风险都远大于收益。
--   本迁移改用 trigger 做引用完整性，与 0020 处理 Team 一致性的方式一致：
--   存在性与同 Team 由 `approvals_permission_link_*` 保证，
--   被引用的策略不可删除则由 0020 的 `permissions_delete_guard` 兜住
--   （该 trigger 无条件拒绝一切 DELETE）。
--
-- ---------------------------------------------------------------------------
-- 为什么 permission_id 允许为 NULL
--
--   存量审批没有这个指向。强制非空等于要求所有历史数据补造关联，
--   那需要规格定义「补不出来时怎么办」——属发明语义。
--   规格（PERMISSION_MATRIX §6）要求 Approval **能**指明对象，不要求**必须**。
--
-- ---------------------------------------------------------------------------
-- 为什么两套 resource_type 词表并存而不统一
--
--   approvals.resource_type（6 值）问的是「这是哪一类高风险动作」，
--   对应 PERMISSION_MATRIX §5 的人类专属动作清单；
--   permissions.resource_type（11 值）问的是「作用在哪个对象上」。
--   有了 permission_id 之后，两者的对应关系由外键直接给出，
--   **不需要**维护一张 11×6 的映射表，也不存在词表对不上导致授权失效的可能。
--   统一词表需改 0009 的 CHECK（已发布迁移，SQLx checksum 不得漂移），
--   且会丢掉「高风险动作类别」这个独立语义。故不改。
-- ===========================================================================

ALTER TABLE approvals ADD COLUMN permission_id BLOB
    CHECK (permission_id IS NULL
        OR (typeof(permission_id) = 'blob'
            AND length(permission_id) = 16
            AND substr(hex(permission_id), 13, 1) = '7'
            AND substr(hex(permission_id), 17, 1) IN ('8', '9', 'A', 'B')));

-- actor_type 沿用 approvals.requester_actor_type 的同一词表，不新增取值：
-- 取消者本质上是「以什么身份取消」，与请求者同构。
-- （events.actor_type 有 ADAPTER 一档，但审批的请求/取消不可能由 Adapter 主张，
--   放进这里会凭空多一个无法解释的取值。）
ALTER TABLE approvals ADD COLUMN cancelled_by_actor_type TEXT
    CHECK (cancelled_by_actor_type IS NULL
        OR cancelled_by_actor_type IN ('ROLE', 'HUMAN', 'SYSTEM'));

ALTER TABLE approvals ADD COLUMN cancelled_by_actor_id BLOB
    CHECK (cancelled_by_actor_id IS NULL
        OR (typeof(cancelled_by_actor_id) = 'blob'
            AND length(cancelled_by_actor_id) = 16
            AND substr(hex(cancelled_by_actor_id), 13, 1) = '7'
            AND substr(hex(cancelled_by_actor_id), 17, 1) IN ('8', '9', 'A', 'B')));

-- ===========================================================================
-- A1：permission_id 的引用完整性
--
-- 存在且同 Team 两条一起判：
--   「存在」防止悬空引用（求交时读到一条不存在的策略）；
--   「同 Team」是因为 permissions.id 是全局唯一主键 —— 外键/存在性检查
--   **挡不住跨 Team**，一条 Team A 的审批完全可以指向 Team B 的策略，
--   而策略里可能含 Team B 的宽范围放行。这与 0020 的 Team 一致性是同一类问题。
--
-- ---------------------------------------------------------------------------
-- 依赖声明（独立核验指出，务必保留）
--
--   上述两个 trigger 都是 `BEFORE UPDATE OF permission_id`，
--   **单独改写 team_id 时它们不触发**。
--   真正兜住「把审批连同其策略指向一起迁到别的 Team」的是 0020 的
--   `approvals_team_consistency_update`，其中第一条是无条件禁令
--   `NEW.team_id IS NOT OLD.team_id`（任何 team_id 改写都被拒）。
--
--   也就是说本迁移的跨 Team 防线**寄生在 0020 上**。当前两种 actor
--   （ROLE / HUMAN）下均实测无绕过，但若将来放宽 0020 那条禁令，
--   这里的 Team 判定会**同时失守**。修改 0020 前请先处理本节。
-- ===========================================================================
CREATE TRIGGER approvals_permission_link_insert
BEFORE INSERT ON approvals
FOR EACH ROW
WHEN NEW.permission_id IS NOT NULL
     AND NOT EXISTS (SELECT 1 FROM permissions AS p
                     WHERE p.id = NEW.permission_id
                       AND p.team_id = NEW.team_id)
BEGIN
    SELECT RAISE(ABORT, 'Approval permission link violates Team boundary');
END;

CREATE TRIGGER approvals_permission_link_update
BEFORE UPDATE OF permission_id ON approvals
FOR EACH ROW
WHEN NEW.permission_id IS NOT NULL
     AND NOT EXISTS (SELECT 1 FROM permissions AS p
                     WHERE p.id = NEW.permission_id
                       AND p.team_id = NEW.team_id)
BEGIN
    SELECT RAISE(ABORT, 'Approval permission link violates Team boundary');
END;

-- ---------------------------------------------------------------------------
-- permission_id 一旦指认即冻结：NULL -> 值允许（补上尚未确定的指向），
-- 值 -> 另一个值禁止。
--
-- 依据：第 7 节把「新请求必须新建 Approval」定为终态后唯一的出路；
-- 同理，一条审批**请求的是哪条策略的放行**是该请求的固有属性，
-- 事后换一条策略顶替，等于把「我申请了 A」改写成「我申请了 B」，
-- 而记录本身看不出发生过替换。请求内容不可变的同族理由见 0020
-- 的 approvals_request_immutable_update。
-- ---------------------------------------------------------------------------
CREATE TRIGGER approvals_permission_immutable_update
BEFORE UPDATE OF permission_id ON approvals
FOR EACH ROW
WHEN OLD.permission_id IS NOT NULL
     AND NEW.permission_id IS NOT OLD.permission_id
BEGIN
    SELECT RAISE(ABORT, 'Approval permission link is immutable');
END;

-- ---------------------------------------------------------------------------
-- 被指向的策略**必须是 ALLOW**。
--
-- 依据：`PERMISSION_MATRIX` §2 有效权限算法末句明文 —
--   「Approval 只能缩小或临时补充**已允许的**动作，不能突破 Team 安全上限」。
--
-- 也就是说 Approval 的作用是「在 Team 上限之内，为某个本已获准但高风险的动作
-- 补一次 Human 确认」，而不是「把一条 DENY 变成 ALLOW」。
-- 若允许审批指向 effect='DENY' 的策略，应用层只要看到
-- 「有一条 Approval 指向该策略」就可能据此放行 ——
-- 那等于让审批**突破 Team 安全上限**，正是 §2 禁止的那件事。
--
-- 独立成条 trigger 而不并入上面的 link 守卫：
-- 「跨 Team」与「指向 DENY」是两种不同的错误，混在一起会让排查时
-- 只看到一个笼统的文案。错误码也不复用。
-- ---------------------------------------------------------------------------
CREATE TRIGGER approvals_permission_effect_insert
BEFORE INSERT ON approvals
FOR EACH ROW
WHEN NEW.permission_id IS NOT NULL
     AND (SELECT p.effect FROM permissions AS p WHERE p.id = NEW.permission_id) IS NOT 'ALLOW'
BEGIN
    SELECT RAISE(ABORT, 'Approval cannot be granted against a denied policy');
END;

CREATE TRIGGER approvals_permission_effect_update
BEFORE UPDATE OF permission_id ON approvals
FOR EACH ROW
WHEN NEW.permission_id IS NOT NULL
     AND (SELECT p.effect FROM permissions AS p WHERE p.id = NEW.permission_id) IS NOT 'ALLOW'
BEGIN
    SELECT RAISE(ABORT, 'Approval cannot be granted against a denied policy');
END;

-- ===========================================================================
-- B2：取消 actor
--
-- 三条独立的不变量，各自管一件事：
--   1) 只有 CANCELLED 才谈得上「被谁取消」   -> cancellations_status
--   2) type 与 id 必须同进同出；ROLE 要落到本 Team 的真实 Role -> cancellations_identity
--   3) 落定后不可改写、不可抹除             -> cancellations_immutable
--
-- 第 3 条用「终态双向冻结」，与 0020 处理决定痕迹同一手法：
--   CANCELLED 之后把 actor 换成别人 = 伪造取消者；抹成 NULL = 抹掉取消者。
--   两个方向都是审计伪造，所以都拦。
--
-- ---------------------------------------------------------------------------
-- 已知边界：**补记也被一并封死**（核验阶段实测确认）
--
--   先 `UPDATE status='CANCELLED'` 再补 `cancelled_by_*`（两步走）会被拦；
--   同一次 UPDATE 内写 `status` 与 `cancelled_by_*`（规格要求的路径）畅通。
--
--   这是**有意为之**，不是遗漏：
--     1. §7 把「记录取消 actor」写在该次**迁移的副作用**列 ——
--        规格要求的是迁移时记录，路径 (b) 正是规格要求的路径；
--     2. 若为补记开口子，等于给审计记录开一个「事后写入无凭据值」的窗口 ——
--        任何拿到已 CANCELLED 审批 id 的人都能补上任意「取消者」，
--        DB 无法判断该补写是否属实，与 §7「决定不可改写」直接冲突。
--
--   对下游的硬约束（DEV-02-12 必须遵守）：
--     `PENDING -> CANCELLED` 必须在**同一次 UPDATE** 内一并写入
--     `cancelled_by_actor_type` / `cancelled_by_actor_id`。
--
--   存量的处置建议：可信的取消者信息若存在，应在 `events` 表里 ——
--   `subject_type` 含 `APPROVAL`、`actor_type`/`actor_id` 齐备，
--   §7 也规定该迁移要发 `APPROVAL_CANCELLED` 事件。
--   回填应走应用层从事件流读取可信 actor，而不是让 DB 接受无来源的手写值。
-- ===========================================================================
CREATE TRIGGER approvals_cancellations_status_insert
BEFORE INSERT ON approvals
FOR EACH ROW
WHEN (NEW.cancelled_by_actor_type IS NOT NULL OR NEW.cancelled_by_actor_id IS NOT NULL)
     AND NEW.status != 'CANCELLED'
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_PRECONDITION_FAILED');
END;

CREATE TRIGGER approvals_cancellations_status_update
BEFORE UPDATE ON approvals
FOR EACH ROW
WHEN (NEW.cancelled_by_actor_type IS NOT NULL OR NEW.cancelled_by_actor_id IS NOT NULL)
     AND NEW.status != 'CANCELLED'
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_PRECONDITION_FAILED');
END;

CREATE TRIGGER approvals_cancellations_identity_update
BEFORE UPDATE ON approvals
FOR EACH ROW
WHEN (NEW.cancelled_by_actor_type IS NULL) <> (NEW.cancelled_by_actor_id IS NULL)
  OR (NEW.cancelled_by_actor_type = 'ROLE'
      AND NOT EXISTS (SELECT 1 FROM roles AS r
                      WHERE r.id = NEW.cancelled_by_actor_id
                        AND r.team_id = NEW.team_id))
BEGIN
    SELECT RAISE(ABORT, 'Approval cancellation actor violation');
END;

CREATE TRIGGER approvals_cancellations_immutable_update
BEFORE UPDATE ON approvals
FOR EACH ROW
WHEN OLD.status = 'CANCELLED'
     AND (NEW.cancelled_by_actor_type IS NOT OLD.cancelled_by_actor_type
          OR NEW.cancelled_by_actor_id IS NOT OLD.cancelled_by_actor_id)
BEGIN
    SELECT RAISE(ABORT, 'Approval cancellation actor is immutable');
END;
