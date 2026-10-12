# AF-03 结果：签回最终证据口径与当前开放项（2026-10-09）

**包**：AF-03（R1 资料 / R3 结论）｜**类型**：纯文档 + 独立重放脚本｜**依赖**：DEV-01-05 / DEV-02-11，关联 DEV-01-08 / DEV-02-08

**结论：六条主控裁决已逐条落成最终证据口径；开放项清单按四类重写；独立重放 38/38 匹配、exit 0。本包不自判 DONE，整批准入仍待 Codex 复审。**

---

## 1. 输入与写集

| 项 | 值 |
|---|---|
| HEAD | `1a14b8795e76fc373bef326eb5df96f9ac173f71`（分支 `codex/wave-2-ci-validation`） |
| 工作树 | 共享 dirty worktree（约 150 项继承改动）**原样保留**；本包**未执行** reset / stash / clean / checkout |
| 权威任务表 | `docs/coordination/MINIMAXCODE-AUDIT-FOLLOWUP-2026-10-09.md` `## AF-03` |
| 权威审计 | `docs/reviews/FC-RETURN-AUDIT-2026-10-09.md`（§3 / §5 / §7 为主） |
| 本轮主控证据 | `docs/reviews/evidence/fc-return-audit-2026-10-09/` |

**新增文件（穷举，未超出写集）：**

- `docs/coordination/minimax-audit-next/AF-03-final-acceptance-index.md`
- `docs/coordination/minimax-audit-next/AF-03-RESULT.md`（本文件）
- `docs/reviews/evidence/minimax-audit-next/AF-03/replay_af03_final_acceptance.py`
- `docs/reviews/evidence/minimax-audit-next/AF-03/replay-output.json`
- `docs/reviews/evidence/minimax-audit-next/AF-03/replay-output.log`

**未改动**：`crates/**`、`scripts/**`、`Cargo.toml`、`Cargo.lock`、`eslint.config.mjs`、`sources/**`、迁移 `0001`–`0029`、全部历史报告与 evidence、`docs/coordination/minimax-followup/*`、`docs/adr/0007-*`（仍 **Proposed**）。

---

## 2. 六条裁决落点

| # | 裁决 | 落点（`AF-03-final-acceptance-index.md`） | 重放项数 |
|---|---|---|---|
| 1 | R10 口径：15 重建 ＋ M14 不可编译/「2 tests」未证；**不能写原声明 16/16**；新增 M14-R 共 16 项 16/16 | §1，两列分列表 ＋ 新旧编号映射表 | 12 |
| 2 | 旧开放清单四类更新 | §2（2.1 已由 0028 关闭 / 2.2 其他已关闭 / 2.3 本轮签收与已验证 / 2.4 仍然开放） | 2 |
| 3 | FC-06 信任前提更正 | §3 | 2 |
| 4 | `RAISE(IGNORE)` = fail-silent 更正，旧记录不覆盖 | §4 | 0（口径性，无可派生数值） |
| 5 | 覆盖率 91.76% / application 占位 / branch 未验证；AF-01 不改 domain 故不重测 | §5 | 8 |
| 6 | 最终数字分「当时存量」与「未来终值（AF-01 后）」两栏 | §6 | 14 |

**合计 38 项，全部匹配（exit 0）。**

裁决 1 的新旧编号映射（A 列 15 项 + 仅 A 列的原写法 M14 + 仅 B 列的 M14-R）已在索引 §1 表格中给出；旧 `mutation/ledger.json` 中 `id = "M14"` 的行即 **M14-R**，`M14-original-result.json` 为**原写法**，两者编号相同内容不同 —— 此映射即为此而设。**旧补丁与原始日志保持原样。**

---

## 3. 验收命令与实跑结果

唯一新增可执行件是纯读重放脚本。命令与退出码：

```
python3 docs/reviews/evidence/minimax-audit-next/AF-03/replay_af03_final_acceptance.py
→ exit 0
```

实跑输出（节选，完整见 `replay-output.log`）：

```
AF-03 independent replay of the six master-control rulings
repo HEAD: 1a14b8795e76fc373bef326eb5df96f9ac173f71
verdict: REPLAY_MATCHES_RECORDED
checks: 38/38 matched
```

**未运行（且不以缓存或 dry-run 冒充）**：完整 canonical Mac 21；联网 advisory / `pnpm install` / `cargo audit`；真实 Harness / Keychain / 备份恢复；最低 macOS 14 实机；A11y；性能；branch 覆盖率；application 真实代码覆盖（占位）；全产品覆盖；在线 `pre-commit`。

**未重新生成任何日志**：643/36/25/8 等数字全部由脚本从本轮既有日志**重新求和**得到，脚本只读、不写既有 evidence。

---

## 4. 正对照 / 真实 RED / 回退

| 项 | 状态 |
|---|---|
| 正对照 | 重放脚本以登记值为期望；**任一派生值不符即 `DIVERGENCE` 且 exit 1**。本次 38/38 匹配 |
| 真实 RED | AF-03 自身 0 条代码变更，无行为改动可产生 RED。AF-04 类 `%` 反例的 RED 由 AF-01 包负责，本包只引用本轮既有 `percent-regression-red.log`（0 passed / 3 failed，exit 101） |
| 回退 | 本包为**纯新增文档与只读脚本**，无回退需求；删除本包 5 个文件即可完全回退，不影响任何既有产物 |
| 正/负分离 | 脚本判据纪律：不能从既有产物派生的项**报为错误而非通过**；编译失败**不计为 kill**；lint 计数取 eslint 机器可读汇总行而非逐条文本 |

**实测到的一次自身失败（如实登记）**：首轮重放 `checks 0/0` —— `expectations` 被当作普通子树展平，校验集为空。第二轮 35/36，`lint-baseline errors` 由 2 误报 —— 用 `\berror\b` 计数把逐条 finding 文本与汇总行各数了一次。第三轮 38/38，`exit 0`。两处均为脚本判据缺陷，已修正为「取 eslint `✖ N problems` 汇总行」与「从 ruling 体中剥离 `expectations` 后比对」。

---

## 5. 权限与节点状态

| 项 | 状态 |
|---|---|
| 自判 DONE | **无。** 正式节点 DONE 权限属 Codex |
| 节点状态 | **20 DONE / 21 BLOCKED / 60 PLANNED，与本轮开始时完全一致。** 本轮**未恢复任何正式节点 DONE**，**未启动任何依赖节点**（`node-status-proof.json`：`node_source_sha256 dd16ad19eb9e5a42bd4af0c24e8f1bfbff1246c19a3671fd26928806e5be8cde`） |
| ADR | `docs/adr/0007-*` 仍 **Proposed**，未改动 |
| 通知 | B-01～B-08 仍为 **Human 待决输入**；未自改字典、未实现待决语义 |
| 后续节点 | K1 可信身份 / K2 已冻结通知身份 / K3 应用事务·幂等·Event·Outbox / K4 dispatcher·Run·fencing / Workspace·IPC·UI：**仍按前置 DONE 与具体写集另签，本表不自动开工** |
| 术语 | 本包不再把 FC-08 称为任务表「第 10 包」——**本轮实际为九包（FC-01…FC-09）** |

---

## 6. 失败项 / 未验项

- **失败项：无。** 本包 0 条失败。
- **未验项（不可豁免）**：AF-01 唯一行为阻断未修；AF-02 存量业务行升级与完整三轴语义联合验收未完成；branch 85% 门槛 UNVERIFIED；application 覆盖为占位；完整 Mac 21 与联网 advisory 未运行（须 Human 明确授权）；统一节点门禁 G0～G9 未完成。

> 现有离线套件全绿**不能豁免** AF-01 或正式节点 G0～G9。

---

## 7. 需裁决 / 需回传 Codex 的开放项

1. **AF-01 修复后 Rust 总数须重新登记**：§6.2 明确「不得硬编码保留 643」。请在收口轮以真实日志更新，勿沿用存量。
2. **branch instrumentation 授权**：Gate C 85% branch 门槛当前 UNVERIFIED。是否另签一个「为 domain 引入有效 branch instrumentation 并重测」的任务？本包不自行扩大写集。
3. **AF-02 联合验收范围**：存量业务行升级 + 完整三轴语义的验收装置是否即 AF-02 交付面，抑或需 Codex 单独签发写集。
4. **联网 Mac 21 / advisory**：仍需 Human 明确授权。未授权前只交回完整离线证据，本包不自行安装或下载。
5. **正式节点准入**：本包不复审任何节点 DONE/BLOCKED 变更。

---

## 8. 费用与合规

全程 **offline**，只用已有工具，**新增费用 $0**。未提交、未推送、未部署、未联网、未做生产操作。未新增依赖。**未要求回写旧 FC-08 原件**，未覆盖或删除任何此前自陈与原始日志。