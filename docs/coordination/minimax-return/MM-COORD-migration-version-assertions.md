# MM-COORD｜跨包写集冲突登记与裁决（2026-10-08）

本文件登记一个**审计与任务表均未预见**的协调缺口，以及 Human 的裁决。它不修改任何测试期望、不扩张任何任务包写集，仅作为 Codex 复核时的裁决痕迹。

## 1. 冲突事实

MM-R01 按任务包要求合规新增 `0025_message_routing_team_integrity.sql` 后，以下两处**既有**测试失败：

| 文件:行 | 断言 | 实测 |
|---|---|---|
| `crates/persistence-sqlite/tests/notification_execution_integrity.rs:470` | `assert_eq!(MAX(version) FROM _sqlx_migrations, 24)` | `left: 25, right: 24` |
| `crates/persistence-sqlite/tests/outbox_default_forward_migration.rs:150` | 同上 | `left: 25, right: 24` |

本端独立实跑 `cargo test -p persistence-sqlite --locked --offline --no-fail-fast` 复核：**恰好 2 处失败，且均为纯版本计数断言**。两处失败点之前的全部行为断言（含 `attention_items.status == "RESOLVED"`）均已通过。R01 自身判定为 `SELF_CHECK_BLOCKED`，拒绝越界修改，判定正确。

## 2. 为什么这是任务表的系统性缺口而非单点疏漏

任务表要求数据库组串行新增三个迁移：**0025（R01）、0026（R02）、0027（R03）**。任何按包执行且不扩写集的执行端，都会让这两处断言在 0025、0026、0027 各失败一次。这是写集划分与测试写法之间的固有冲突，审计的分片复核未覆盖（审计 §2 记录的是 `cargo test --workspace` 在 0025 落地**之前**的结果 503 passed / 0 failed）。

全仓扫描结论（供 Codex 复核时直接采信）：

- `MAX(version)` 断言共 4 处，**只有上述 2 处脆弱**（语义为「最新版本」）。
- 另 2 处（`notification_execution_integrity.rs:424` 期望 `22`、`outbox_default_forward_migration.rs:245` 期望 `23`）是**回滚断言**，语义为「迁移失败后停在原地」，**不受新迁移影响，无需改动**。
- 其余迁移断言一律使用稳健写法 `WHERE version = N AND success=TRUE`（`version = 10/11/12/13/14/15` 共六处前向迁移测试均如此）。仓内已存在正确范式，脆弱写法是这 2 处的局部偏离。

## 3. 已否决的方案及理由

- **把 `24` 改成 `25`**：改动最小但**必然被 0026、0027 再撞两次**，等于把同一问题推迟两次并留下三处待改。
- **由本端擅自修改**：违反任务表「未列文件只读」「任何新增/冲突写集先回传，不擅自扩展」，且「改预期值来保绿」是任务包明令禁止的行为。

## 3b. 第三处：MM-R02 自身引入的同类断言（总控漏检，如实登记）

MM-R03 执行期间发现**第三处**版本计数失败，本端独立复核**成立**：

| 文件:行 | 断言 | 实测 |
|---|---|---|
| `crates/persistence-sqlite/tests/core_history_retention.rs:1275` | `assert_eq!(COUNT(*) FROM _sqlx_migrations WHERE success=TRUE, 26)` | 0027 落地后为 `27`，失败 |

- 该测试名为 `this_package_owns_exactly_version_26`，其**意图**是「本包注册的 26 号迁移已成功应用」。
- 但它用的 `COUNT(*) WHERE success=TRUE` 统计的是**全部**成功迁移数，与 `MAX(version)` 是同一类脆弱形式。
- 讽刺的是，**同一文件 `:1169`** 用的就是稳健形式 `version=26 AND success=TRUE`；MM-R03 自己的新测试也全部使用 `version = 27 AND success = TRUE`。

### 总控漏检声明

本端在 MM-R02 复核时**只扫描了 `MAX(version)` 形态**，未覆盖 `COUNT(*) ... WHERE success=TRUE`（无 version 限定）这一形态，因此漏掉了这处，并误向 R03 声明「只有 2 处已登记失败」。**这是本端的复核失误，不是执行端问题。** MM-R03 主动发现并如实上报。

MM-R03 所在包无法修改该文件（不在其写集内），已按停止条件拒绝越界，正确。

### 与已裁决两处的性质差异（请 Codex/Human 注意）

| | 已裁决的 2 处 | 本处 |
|---|---|---|
| 来源 | 上一轮遗留的**历史**测试 | **本轮 MM-R02 新写入**的测试 |
| 是否被裁决覆盖 | 是（Human 明确裁决「留给 Codex」） | **否**，当时尚不存在 |
| 所在包写集 | 不属任何包 | **属于 MM-R02 的写集**（`core_history_retention.rs`） |
| 最小修法 | 改稳健形式 | 同左，且**同一文件 `:1169` 已有可直接照抄的稳健范式** |

因此本处不属于「不得扩写集」的情形——修改它**不构成写集扩张**，只是回到 MM-R02 自己的写集内修正一处自身引入的断言。但本端仍**未擅自执行**，因为：① 该包已关闭并标记 `SELF_CHECK_PASS`，重启需明确授权；② 保持「凡计数断言一律交 Codex」的一致性比破例更重要。

建议最小修法：`WHERE success=TRUE` → `WHERE version = 26 AND success = TRUE`，语义与测试名一致。

## 4. Human 裁决（2026-10-08）

**选择：保持写集不扩张，不修改这两处断言，留给 Codex 在最终统一门禁中处理。**

据此本端执行方式：

- R01 / R02 / R03 照常按包执行，不因这两处失败停工、不改期望值、不加 `#[ignore]`。
- 这 2 处失败在 0025/0026/0027 落地后**预期**表现为 `left: 26, right: 24`、`left: 27, right: 24`。
- 每次回传 RESULT 时，把「除这 2 处已登记的版本计数断言外全绿」作为口径明确写出，**不得**表述为「全工作区通过」。

## 5. 交回 Codex 的最小修法建议（供签发写集时参考，本端未执行）

将两处改为语义等价且不随新迁移漂移的稳健形式，与仓内既有范式一致：

```rust
// 由：SELECT MAX(version) FROM _sqlx_migrations  == 24
// 改为：SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 24 AND success = TRUE  == 1
```

该形式断言的语义（「0024 已成功应用」）才是这些前向迁移测试真正要验证的内容，且 0026/0027 落地后无需再次修改。此建议仅登记，未执行；是否签发写集由 Codex 决定。

## 6. 相关回传

- `docs/coordination/minimax-return/MM-R01-RESULT.md`（R01 判定 `SELF_CHECK_BLOCKED`，含其自陈的未覆盖项）
- `docs/reviews/evidence/minimax-return/MM-R01/`（含 `--no-fail-fast` 全量结果与断言差异）