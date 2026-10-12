# FC-08 勘误一：撤回「审计 artifacts 只给 11 个值，故原 RED 不可复现」这条指控

- 包：FC-08（01-05、02-11 的文档/变异/覆盖证据整理）
- 起草：Minimaxcode Worker 子会话，2026-10-09
- 输入：HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`，分支 `codex/wave-2-ci-validation`，共享 dirty worktree 未 reset/stash/clean
- 依据：[独立审计 §5](../../reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md#5-对审计自身-artifacts-装置缺陷的裁决)、[接续任务表 FC-08 第 1 条](../MINIMAXCODE-FOLLOWUP-2026-10-08.md)
- 证据目录：`docs/reviews/evidence/minimax-followup/FC-08/errata/`

> **本文件不删除、不改写任何旧文字。** 下面逐条引用的是旧文档中的**原句**，旧文档本身一字未动。撤回通过本勘误表达。

---

## 1. 被撤回的指控，逐项定位

共 5 处旧文字提出同一指控。下表给出文件、行号与原文首句（完整原文留在原处，不在此复刻，避免二次抄写出错）。

| # | 文件 | 行 | 原句（节录） |
|---|---|---|---|
| 1 | `docs/coordination/minimax-return/MM-R02-RESULT.md` | 167 | 「审计提交的复现件 `audit_storage_boundaries.rs` 中，`artifacts` 分支的 INSERT 只给了 **11 个值**，而 `0005` 的 `artifacts` 表有 **12 列**……该语句在当前 schema 上必然报 `table artifacts has 12 columns but 11 values were supplied`」 |
| 2 | 同上 | 169 | 「**结论：审计那份 RED 日志的 `artifacts` 一条在本仓库上不可复现，属审计装置缺陷**」 |
| 3 | 同上 | 267 | 风险项 3：「§5.1 记录的 `artifacts` 向量列数问题使审计 RED 日志该条不可复现」 |
| 4 | `docs/coordination/minimax-return/MM-R03-RESULT.md` | 257 | 「审计 `audit_storage_boundaries.rs` 的 `artifacts` 分支 INSERT 只给 11 个值，而 `0005` 的 `artifacts` 表有 12 列」（自述「沿用 MM-R02 的独立确认」） |
| 5 | `docs/coordination/minimax-return/MM-COORD-review-gate-2026-10-08.md` | 80–82 | §B-2 原文三条，现已由该文档自身的撤回声明折叠留痕（见 §4） |

**这 5 处全部撤回。** 指控的两个支点——「INSERT 只给 11 个值」与「因此该条 RED 不可复现」——经本轮独立实测，两条都不成立。

---

## 2. 撤回依据一：`artifacts` 表是 12 列，不是 11 列

由**原 main-repro 的 24 份迁移**在真实 SQLite 中建库后，用 `PRAGMA table_info(artifacts)` 直接读出，不靠肉眼数 `CREATE TABLE`：

```
migration files applied : 24
PRAGMA table_info(artifacts) -> 12 columns

  #  column
   1 id                    5 creator_role_id    9  git_commit_sha
   2 team_id               6 artifact_type      10  visibility
   3 task_id               7 uri                11  version
   4 run_id                8 checksum_sha256    12  created_at
```

复现：`python3 docs/reviews/evidence/minimax-followup/FC-08/errata/derive_artifacts_value_count.py`
产物：`errata/artifacts-value-count.txt`

**顺带记录本轮自己踩到的一个同类坑**：我第一次用一个「按 `^    [a-z_]+ ` 抓列名」的正则去数同一张表，得到的正是 **11**——它漏掉了 `checksum_sha256`。也就是说，**一个看起来完全合理的机械计数，在这张表上同样会给出 11**。这解释了旧指控的成因，也说明单靠任何一种数法都不足以定案；必须让数据库自己回答。

---

## 3. 撤回依据二：原 INSERT 逐值解析为 12 个值

对 `main-repro/crates/persistence-sqlite/tests/audit_storage_boundaries.rs:55` 的原始语句做顶层逗号切分（引号与 `X'…'` 内的逗号不计），得 12 个值：

| # | 列 | 值 |
|---|---|---|
| 1 | id | `X'01921389d31174628f80cb5f79ab0010'` |
| 2 | team_id | `X'01921389d31174628f80cb5f79ab0001'` |
| 3 | task_id | `X'01921389d31174628f80cb5f79ab0006'` |
| 4 | run_id | `X'01921389d31174628f80cb5f79ab0007'` |
| 5 | creator_role_id | `X'01921389d31174628f80cb5f79ab0002'` |
| 6 | artifact_type | `'TEST_REPORT'` |
| 7 | uri | `'tasks/T-1/report'` |
| 8 | checksum_sha256 | `?` — 绑定参数，**仍然是一个值** |
| 9 | git_commit_sha | `NULL` — **显式 NULL** |
| 10 | visibility | `'TEAM_INTERNAL'` |
| 11 | version | `1` |
| 12 | created_at | `5` |

**12 values vs 12 columns → MATCH。**

漏数的位置有两处，两处都极其容易看漏：

1. 第 8 位的 `?` 是**绑定参数**。它长得不像「值」，但 `INSERT INTO t VALUES(?, ...)` 里 `?` 就是占掉一列的值。原 R02 自己的复现件用的是 `.bind("0".repeat(64))`。
2. 第 9 位的**显式 `NULL`** 夹在 `checksum_sha256` 与 `visibility` 之间，位置很不起眼。原 R02 的描述其实**已经点出了这一列的位置**（「`git_commit_sha` 位于 `checksum_sha256` 与 `visibility` 之间」），却又断言只给了 11 个值——这两句彼此矛盾，矛盾就出在没数上那个 `NULL`。

---

## 4. 撤回依据三：本轮用 SQLx 重放的原 RED

独立于 Codex 已有的回放，本轮在**自己的副本**上重跑了一次。

- 副本：`errata/red-replay/`，由原 `docs/reviews/evidence/deep-audit-2026-10-08/main-repro` 整树复制而来，**129 个文件逐字节 SHA-256 全部一致**（`identical: True`）。
- 被测文件 SHA-256：`6e2e1d2741527ea3644bdfc524ebe1c0e44016df88fa2951bae3f32c5811d8ef`（与原文件一致）。
- **全新 target 目录**：`errata/red-replay-target/`，不复用任何既有构建产物。
- 命令：`cargo test --manifest-path errata/red-replay/Cargo.toml -p persistence-sqlite --test audit_storage_boundaries --locked --offline --no-fail-fast`
- 退出码：**101**；原始日志：`errata/red-replay.raw.log`（未过滤）

结果：

```
test result: FAILED. 1 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.82s

---- retained_histories_cannot_be_physically_deleted stdout ----
panicked at crates/persistence-sqlite/tests/audit_storage_boundaries.rs:67:5:
retained history DELETE accepted for ["runs", "reviews", "artifacts", "approvals", "messages"]
```

两点必须讲清楚：

1. **`artifacts` 出现在被删名单里。** 与 Codex 独立回放（`1 passed / 3 failed`，同样含 artifacts）结论一致。DA-02 的结论成立。
2. **日志中列数不匹配的错误出现次数为 0**（`grep -c 'has 12 columns'` → `0`）。原 R02 断言「必然报 `table artifacts has 12 columns but 11 values were supplied`」——这句话在实测里根本没有发生。

而且 panic 行号本身就是反证：`:55` 是 INSERT，`:64` 是 `assert_eq!(COUNT(*) …, 1)`，`:67` 才是 DELETE 后的断言。**测试能走到第 67 行，说明第 55 行的 INSERT 成功执行了。** 若是列数不匹配，SQLx 会在 `.execute(&pool).await?` 处即返回错误，测试会停在第 55 行附近并报 SQLx 错误，而不是走到第 67 行报「五表全可删」。

### 与 `MM-COORD-review-gate` 自身撤回声明的关系

该文档 §B-2 在 2026-10-08 已由本端自查后自行撤回，原文用 `<details>` 折叠保留，**未覆写**。本勘误**不是**对该撤回的再次撤回，而是：把 R02/R03 里**仍然留在正文、仍然当作事实陈述**的同一条指控一并结清，并为整条链路补上本轮的独立机器证据。

---

## 5. 继续保留、不在撤回范围内的真发现

撤回范围**只有**「11 值 / 不可复现」这一条。以下两项是真实缺陷，**继续有效**，本勘误不削弱、不订正、不降级：

| 发现 | 冻结依据 | 现状 |
|---|---|---|
| `"references"` 表在 `0001`～`0025` 无任何 `BEFORE DELETE` 守卫 | `DATA_CONVENTIONS §5`：引用失效只更新 `validation_status`，不删除历史 URI/path/hash | 真实缺口。审计 §3 DA-02 称「`0013` 的 Reference guard 已存在且会保护有关联的行」——就 INSERT/UPDATE 成立，**不覆盖 DELETE** |
| `delivery_attempts` 表在 `0001`～`0027` 无任何 `BEFORE DELETE` 守卫 | `DATA_CONVENTIONS §5`、`§6` 双处明列 | 真实缺口。表上十余个触发器全部是 `BEFORE INSERT` / `BEFORE UPDATE` |

Codex 独立审计 §3 MR-03、§5 末句均确认这两项保留。审计 §5 已自陈：「其余真实 refs/DeliveryAttempt 发现保留，不因这条错误全部否定」——本勘误与之一致。

另需一并说明：本条错误指控**不能**用来订正审计的其他有效结论。审计 §5 的裁定原文即为「不能用它订正原审计有效结论」，本勘误照此执行。

---

## 6. 成因与残留影响（如实登记）

- **成因**：读一行很长的 SQL 时漏看了 `NULL`，且未自行计数就采信了 MM-R02 的转述，随后把「已独立核实成立」写进了 COORD 清单。`MM-COORD-FC-review-2026-10-08.md:61` 已认领该轮错误，本勘误确认认领成立。
- **残留**：MM-R02 §5.1 与 §风险 3、MM-R03 风险 5 中的该条指控**原文仍在**，仍会误导后来的读者。修订这两份历史 RESULT 超出 FC-08 写集（它们属历史归档），故按任务包要求以本勘误结清，**不改旧文件**。
- **未做**：没有、也不打算去「修正」原审计的 `storage-boundaries-red-v2.log`。该日志记录的现象经本轮回放属实（五表全可删），它本来就不是错的。

---

## 7. 费用与边界

全部命令 `--locked --offline`，`CARGO_NET_OFFLINE=true`；无下载、无联网、无外部账号、无新增依赖、未提交/推送/部署、未操作生产数据。费用恒为 **$0**。只读复制的历史 evidence 未被修改。