# FC-01 结果：三处迁移版本计数断言整改（R1）

- **包**：FC-01（DEV-02-05 / 02-09 / 02-14）
- **执行端**：Minimaxcode，模型 `MiniMax-M3.1-Flash-Preview`（实际模型由执行端回填，未代填）
- **日期**：2026-10-08
- **输入 HEAD**：`1a14b8795e76fc373bef326eb5df96f9ac173f71` + 共享 dirty worktree
- **依据**：[独立审计 §2 与 §3 MR-04](../../reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md)、[接续任务表 FC-01 详细要求](../MINIMAXCODE-FOLLOWUP-2026-10-08.md)、[MM-COORD 跨包登记](../minimax-return/MM-COORD-migration-version-assertions.md)
- **环境**：`PATH=/Users/chenzhiqing/.minimax/toolchain/bin:$PATH`、`CARGO_NET_OFFLINE=true`；全部 cargo 命令 `--locked --offline`（漂移实证的隔离副本亦为 `--locked --offline`）；未联网、未安装、未新增依赖、未提交/推送/部署
- **自检**：`SELF_CHECK_PASS`（本包范围内）。自检不等于 DONE，正式节点状态与最终准入由 Codex 判定。

## 1. 结论

三条计数断言全部改为**「该迁移的成功记录恰有一条」**的版本限定形式。当前三目标 **37 passed / 0 failed，exit 0**；`persistence-sqlite` 全套 **exit 0**。两处故意失败升级的回滚证明（22 / 23）**未改动且仍绿**。

隔离副本实证：新增一条 0028 迁移后（链长 28），这三处断言**全部仍绿**；同副本内回退成旧形态则三条**全部失败**（`left: 28`），证明该实证非空转。

## 2. 预先记录的三条原失败（RED，修改前）

命令：

```
cargo test -p persistence-sqlite --test notification_execution_integrity \
  --test outbox_default_forward_migration --test core_history_retention \
  --locked --offline --no-fail-fast
```

**exit 101**，3 个 target 失败。日志：[`01-baseline-three-targets.log`](../../reviews/evidence/minimax-followup/FC-01/01-baseline-three-targets.log)

| # | 测试 | 位置 | 失败体 |
|---|---|---|---|
| 1 | `this_package_owns_exactly_version_26` | `core_history_retention.rs:1274` | `assertion left == right failed / left: 27 / right: 26` |
| 2 | `a_later_task_block_does_not_invalidate_historical_attention_resolution` | `notification_execution_integrity.rs:470` | `assertion left == right failed / left: 27 / right: 24` |
| 3 | `upgrade_preserves_all_rows_and_changes_only_the_column_default` | `outbox_default_forward_migration.rs:146` | `assertion left == right failed / left: 27 / right: 24` |

三处失败前的业务行为断言均已通过（含 `attention_items.status == "RESOLVED"`）。

## 3. 普查（不只扫 `MAX(version)`）

全量普查表见 [`MIGRATION-COUNT-CENSUS.md`](../../reviews/evidence/minimax-followup/FC-01/MIGRATION-COUNT-CENSUS.md)。活跃代码共 **22 处** `_sqlx_migrations` 断言，检索词含 `MAX(version`、`COUNT(*) ... WHERE success=TRUE`、`COUNT(*) ... WHERE version=N`、`description ... WHERE version=N`，以及 `migrations().len()` / `migrator.*count`（阴性）。链长 **27**。

分类处置：

| 类别 | 数量 | 处置 |
|---|---|---|
| 链长度脆弱形态（语义是「本迁移已应用」） | 3 | **已修** |
| 故意失败升级的回滚证明（`MAX == 22` / `MAX == 23`） | 2 | **不动**，仍绿 |
| 已是版本限定稳健形式 | 17 | **不动** |

**上一轮漏检的形态已在本次覆盖**：本端上一轮只扫了 `MAX(version)`，漏掉无 version 限定的 `COUNT(*) WHERE success=TRUE`（正是 `core_history_retention.rs:1275`）。本次按形态逐类检索，`COUNT(*) ... WHERE version=N`（负向 0 行证明）、`description WHERE version=N` 也一并核过。

## 4. 逐文件修改

三处均为**同形替换**：把「链长度」断言换成「该版本成功记录恰一条」，并补注释说明为何不用 `MAX(version)`。业务行为断言一律原样保留。

### 4.1 `crates/persistence-sqlite/tests/notification_execution_integrity.rs:471`

```diff
-        query_scalar::<_, i64>("SELECT MAX(version) FROM _sqlx_migrations")
-            .fetch_one(&pool)
-            .await?,
-        24
+        query_scalar::<_, i64>(
+            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 24 AND success = TRUE"
+        )
+        .fetch_one(&pool)
+        .await?,
+        1,
+        "0024 did not record exactly one successful application"
```

测试意图：`db_at(Some(22))` 起库 → `migrate()` 两次 → 验证「后来的 TASK 阻断不改写历史 attention 已 RESOLVED」。`MAX(version)==24` 是在验证**这次升级确实跑过了 0024**，不是链长度语义。

### 4.2 `crates/persistence-sqlite/tests/outbox_default_forward_migration.rs:147`

同上替换为 `version = 24 AND success = TRUE == 1`。测试意图：v23 起库、跑完 0024，验证行/守卫/DDL 默认值全部按预期。

### 4.3 `crates/persistence-sqlite/tests/core_history_retention.rs:1275`

```diff
-        query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations WHERE success=TRUE")
-            .fetch_one(&pool)
-            .await?,
-        26
+        query_scalar::<_, i64>(
+            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=26 AND success=TRUE"
+        )
+        .fetch_one(&pool)
+        .await?,
+        1,
+        "0026 did not record exactly one successful application"
```

测试名 `this_package_owns_exactly_version_26` 与紧邻的 `description == "core history retention"` 都表明意图是「0026 这一条已成功应用」。同时修正了已失真的 doc 注释（「the next free one」→ 描述成功记录），**注释不是断言，未改变任何断言强度**。

写集核对：仅改这三个测试文件 + 本包 RESULT + 本包 evidence 目录。**未改**任何迁移、`src/lib.rs`、`Cargo.toml`、`Cargo.lock`、其他测试文件、生产代码、历史 evidence、正式节点状态。`src/lib.rs` 与 `Cargo.toml` 经比对与 MM-R10 副本逐字节相同。未删除、未过滤、未 `#[ignore]` 任何失败用例。

## 5. 修复后实测

验收命令：

```
cargo test -p persistence-sqlite --test notification_execution_integrity \
  --test outbox_default_forward_migration --test core_history_retention \
  --locked --offline --no-fail-fast
```

**exit 0** —— `core_history_retention` 19/0、`notification_execution_integrity` 14/0、`outbox_default_forward_migration` 4/0。日志：[`02-fixed-three-targets.log`](../../reviews/evidence/minimax-followup/FC-01/02-fixed-three-targets.log)

**全套 crate**：`cargo test -p persistence-sqlite --locked --offline --no-fail-fast` → **exit 0**，30 个 target 全 `ok`，0 failed（修复前为 206 passed / 3 failed）。日志：[`03-full-crate-suite.log`](../../reviews/evidence/minimax-followup/FC-01/03-full-crate-suite.log)

**回滚断言仍绿**（要求 3）：

```
test incompatible_identity_upgrade_rolls_back_and_keeps_old_rows ... ok      # MAX==22，未改动
test failed_rebuild_rolls_back_schema_and_keeps_corrupt_history_for_diagnosis ... ok  # MAX==23，未改动
test a_failed_upgrade_has_no_side_effects ... ok                              # COUNT WHERE version=26 == 0
test failed_resolution_rolls_back_prior_source_changes_in_the_transaction ... ok
```

**格式与静态检查**：`cargo fmt -p persistence-sqlite -- --check` → exit 0（`06-fmt-check.log`）；`cargo clippy -p persistence-sqlite --tests --locked --offline` → exit 0，0 warning（`07-clippy.log`）。

## 6. 论证：不再随未来迁移漂移

### 6.1 语义论证

`MAX(version)` 与 `COUNT(*) WHERE success=TRUE` 的值**等于迁移链里成功落地的条数**（链无洞时两者相等，本仓 0001–0027 连续）。因此它们的期望值必须等于「当前链长」，任何新增迁移——哪怕与该测试毫无关系——都会让它们失败。这是**用链长代理「本迁移已应用」**，本质脆弱。

改为 `COUNT(*) WHERE version = N AND success = TRUE == 1` 后，被断言的事实变成**一个只依赖单个迁移编号的局部事实**：

- 新增迁移 0028/0029/… 只在 `_sqlx_migrations` **新增**一行（`version=28/29`），不会新增或修改 `version=24/26` 的行；
- 该查询带 `version = N` 过滤，`version=28` 的行对结果贡献恒为 0；
- 故无论链长变成 27、28、29 还是更大，断言值恒为 `1`。

即：**期望值 `1` 是「恰有一条成功记录」的语义常量，与链长无关**；而被计数的那条记录的身份由 `version = N` 固定。这是仓内既有稳健范式（v10–v15 六处前向迁移测试、以及 MM-R03 的 0027 测试都已如此），本次是把三处局部偏离改回既有范式。

反向也成立：若 0024/0026 未落地、重复落地、或留下 `success=FALSE`，该断言会失败——**强度未被削弱，且比原 `MAX(version)` 更能定位失败原因**（原写法在链长为 27 时会报 `left: 27`，完全看不出是哪个迁移缺失；新写法直接报 `0024 did not record exactly one successful application`）。

### 6.2 实证：隔离副本新增 0028 后三条仍绿

不改动共享仓库，做法：把工作区复制到系统临时目录的**独立副本**，**只在副本里**加一条 `0028_fc01_drift_probe_noop.sql`（建一张空表，无任何行为变化）并在**副本的** `src/lib.rs` 注册为版本 28，然后在副本里跑同样三条测试。共享 `crates/persistence-sqlite/migrations/` 全程未加文件（仍为 27 个 `.sql`，`src/lib.rs` 无 `0028`）。

- 副本：`/tmp/fc01-drift-probe.ev8fDM`（记录于 [`drift-probe-artifacts/probe-path.txt`](../../reviews/evidence/minimax-followup/FC-01/drift-probe-artifacts/probe-path.txt)）
- 合成迁移与注册补丁：[`drift-probe-artifacts/0028_fc01_drift_probe_noop.sql`](../../reviews/evidence/minimax-followup/FC-01/drift-probe-artifacts/0028_fc01_drift_probe_noop.sql)、[`drift-probe-artifacts/probe-lib-rs-0028-registration.diff`](../../reviews/evidence/minimax-followup/FC-01/drift-probe-artifacts/probe-lib-rs-0028-registration.diff)（副本相对共享 `src/lib.rs` 的**全部**差异，共 +9 行）

结果（链长 28）：

```
test result: ok. 19 passed; 0 failed   (core_history_retention)
test result: ok. 14 passed; 0 failed   (notification_execution_integrity)
test result: ok.  4 passed; 0 failed   (outbox_default_forward_migration)
```

其中三条原失败用例与两条回滚用例均 `... ok`。日志：[`04-drift-probe-with-0028.log`](../../reviews/evidence/minimax-followup/FC-01/04-drift-probe-with-0028.log)

### 6.3 反证（证明该实证非空转）

若副本里的断言仍能通过，说明探针本身不敏感、6.2 的绿没有意义。因此在**同一副本**用 [`drift-probe-artifacts/negative-control-revert.py`](../../reviews/evidence/minimax-followup/FC-01/drift-probe-artifacts/negative-control-revert.py) 把三处改回旧形态（脚本只能指向副本，绝不指向共享仓库）后重跑：

```
cargo test ... --locked --offline --no-fail-fast      → exit 101
left: 28  right: 26     (this_package_owns_exactly_version_26)
left: 28  right: 24     (a_later_task_block_does_not_invalidate_historical_attention_resolution)
left: 28  right: 24     (upgrade_preserves_all_rows_and_changes_only_the_column_default)
```

即：链长 28 下旧形态三条全红、新形态三条全绿。漂移修复既被证明有效，也被证明可被检测。日志：[`05-drift-probe-negative-control-fragile-form.log`](../../reviews/evidence/minimax-followup/FC-01/05-drift-probe-negative-control-fragile-form.log)

## 7. 冻结依据

- MR-04：`docs/reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md` §3「三处计数断言应改为验证该迁移成功记录恰一条，不改成最新版本 27，也不删行为断言；故意失败升级的 `MAX(version)=22/23` 回滚证明保留。」
- 接续任务表 FC-01：纠正三条 `MAX(version)==24` / `COUNT(success)==26`；不换 27/29；不过滤失败用例；全仓检索不只扫 MAX。
- 无冻结规格的领域状态/权限/协议语义变更；未改生产代码，故不触发新 ADR。

## 8. 与 MM-COORD 旧裁决的关系

[`MM-COORD-migration-version-assertions.md`](../minimax-return/MM-COORD-migration-version-assertions.md) §4 的 Human 裁决是「保持写集不扩张，留给 Codex」。本轮 Codex 已在接续任务表中正式签发 FC-01 写集并以 MR-04 取代该裁决，故按 MR-04 执行；旧裁决登记的「MAX 共 4 处、只有 2 处脆弱」结论已被本轮全形态普查扩展为 **22 处、3 处脆弱**，以本文与普查表为准。旧文件作为历史裁决痕迹保持只读，未改写。

## 9. 输入输出指纹

HEAD：`1a14b8795e76fc373bef326eb5df96f9ac173f71`（未提交、未推送）

修改后三文件 SHA-256：

```
b8bb9f8c4e4e590683b996951a5b1f3a13d733917a761308f0e3836a53908447  tests/notification_execution_integrity.rs
1ab98843713ee7872eed8a626c6d6e4e4feae5109fd9cc7579a3eb096d694103  tests/outbox_default_forward_migration.rs
1786b3f133aefb14b09f8c65057cbc3d9564f1b8b4ee878db4ec062dd0434ff8  tests/core_history_retention.rs
```

`crates/persistence-sqlite/src/lib.rs` 与 `Cargo.toml`：与 MM-R10 源副本逐字节相同（未改）。`crates/persistence-sqlite/migrations/`：27 个 `.sql`，与本包开工前一致（未加文件）。

## 10. 剩余风险与回退

**剩余风险**

1. `run_transition_integrity.rs:1039-1043` 的文档注释仍写着「two existing tests pin `MAX(version) = 24` … Human's ruling is that they stay as they are」，本轮已使其过期。该文件**不在本包写集**，未改。请 Codex 在整合时一并更新该注释，或随 FC-03 写集处理。
2. 本包只覆盖 `persistence-sqlite` 的断言形态。全仓其他 crate（`adapter-supervisor` 等）未出现 `_sqlx_migrations` 检索命中，判定为不适用，但未逐 crate 复审其自有迁移计数机制。
3. 漂移实证使用**合成 no-op 迁移 0028**。真实 FC-02 的 0028 会新增留存 trigger，链长同样 +1，断言语义不受影响；但本包**未**对 FC-02/FC-03 真实迁移落地后的结果负责——那属于 FC-02/FC-03 的验收。

**回退**：三处均为单点断言替换，回退即恢复原 `MAX(version)` / `COUNT(*) WHERE success=TRUE` 两行（`§4` 给出完整 before/after）。回退后该 crate 恢复为 3 failed 的已知状态，不涉及数据或迁移回滚；因未改任何迁移文件，无前向修复回退风险。

## 11. 未做与不在范围

- 未改任何迁移、未改 `src/lib.rs`、未改生产代码、未改其他测试文件、未改 manifest/lockfile、未改历史 evidence、未改正式节点状态。
- 未提交、未推送、未部署、未联网、未新增依赖或费用。
- 未把本包局部通过当作任何 DEV 节点 DONE；未启动依赖节点。

**`SELF_CHECK_PASS`** — 本包三项验收（三目标 0 failed、回滚断言仍绿、漂移实证含反证）全部完成。正式准入由 Codex 判定。