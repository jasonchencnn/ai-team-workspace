# MM-R04｜修复百分号转义脱敏漏检（DA-04）回传

关联节点 **DEV-01-08**（当前 `BLOCKED`，本包不解除）。缺陷编号 **DA-04**，P1 脱敏风险，R3。
本文件只是执行端自验收回传，**不等于 DONE**，正式节点状态与依赖解除仍由 Codex 裁定。

---

## 1. 实际执行端与环境

| 项 | 值 |
|---|---|
| Harness | MiniMax Code（本地 runtime，macOS darwin 27.0.0 arm64） |
| 实际模型/版本 | `MiniMax-M3.1-Flash-Preview`（执行端回填，不假定为 M3） |
| 仓库 | `/Users/chenzhiqing/Documents/MyVault/协作工作` |
| 共享工作树 | 与 Codex 同一未提交工作树；**全部继承成果保留**，未执行 `reset`/`checkout`/`clean`/`stash` |
| 工具链 | cargo 1.98.0 / rustc 1.98.0 / rustfmt 1.9.0-stable / clippy 0.1.98（`PATH=/Users/chenzhiqing/.minimax/toolchain/bin`） |
| 网络 | 全部命令 `--locked --offline`，`CARGO_NET_OFFLINE=true`；无下载、无外部服务、无新增依赖 |
| 新增费用 | $0；未访问生产凭据或数据；未提交/推送/部署 |
| 只用模拟 secret | 全程只用审计的 `Audit-Only/AaB` 与既有 `wave2-secret-cookie`；未读取、写入或打印任何真实凭据 |

## 2. 输入与输出 SHA

| 项 | 值 |
|---|---|
| HEAD | `1a14b8795e76fc373bef326eb5df96f9ac173f71`（分支 `codex/wave-2-ci-validation`） |
| 审计声明输入 SHA | `8845a053b778b67aeac7a2138ebeaa16432781572cdb2232fa085c30816de37f`（299 个范围文件，非本包执行时最终输入） |
| `src/lib.rs` 修复前 | `9251f0edaa7fc203e7c02ad92cd5293f4325e830ecfc9d7f77d46e749b66091e` |
| `src/lib.rs` 修复后 | `fc1d5bf801c37be327d434655c9be2874b1b2854683222098b419a1b268e7586` |
| `tests/structured_logging.rs` 修复前 | `fcdfa6cade290c6141108a10af1540244214ccc0e85a8d4f96a1b4f61e7ee84e` |
| `tests/structured_logging.rs` 修复后 | `2bed8f18ac9438f3be9e8a5f59c0d1d98a3b00ba740a358a77394545911c8217` |
| 原始 manifest | `docs/reviews/evidence/minimax-return/MM-R04/input-manifest.txt` |

逐行核对：`src/lib.rs` 修复前的 SHA 与审计复现副本
`docs/reviews/evidence/deep-audit-2026-10-08/main-repro/crates/observability/src/lib.rs` **完全相同**，
即本包修的就是审计当时那份字节，不存在"先偷偷改了再声称复现"。

## 3. 缺陷事实与正确语义

`crates/observability/src/lib.rs` 原 `derived_forms()` 生成了两个 percent 形式：

```rust
percent                      // %XX 一律大写：Audit%2DOnly%2FAaB
percent.to_ascii_lowercase() // ← 缺陷：把整个串小写，连 secret 里未转义的字母也改了
```

第二个形式匹配的是 `audit%2donly%2faab`，即**连未转义字母的大小写都被折叠**。于是合法的
`%2d`／`%2D` 混合大小写形式（`Audit%2dOnly%2fAaB`）既不匹配第一个，也不匹配第二个，
已注册的 secret 原样进入日志。

**本包实现的语义**：`%XX` 的两个 hex 位大小写不敏感（`%2d` ≡ `%2D`）；secret 里未转义的字母
必须保留原始字节大小写（`Audit` ≠ `audit`）。

## 4. 逐文件修改说明

写集严格限于任务包所列：`crates/observability/src/lib.rs`、
`crates/observability/tests/structured_logging.rs`，外加本 RESULT 与本包 evidence 目录。
**未新建** `audit_percent_case.rs` 之类的测试文件，测试全部并入 `structured_logging.rs`。

### 4.1 `crates/observability/src/lib.rs`（+230 / −27）

1. **`Redactor.dangerous_forms` 类型**：`Arc<Vec<String>>` → `Arc<Vec<RedactionForm>>`。
   `RedactionForm` 为 `Exact(String)` / `Percent(PercentForm)`，带 `match_len()`（按最长匹配字节
   长度排序，**与原 `sort_by_key(Reverse(len))` 的口径一致**）与 `is_empty()`。
2. **`sanitize()`**：分派。`Exact` 仍走原来的 `String::replace`（plain / hex / base64 行为**逐字不变**）；
   `Percent` 走新的 `PercentForm::replace_in`。
3. **删除** `percent.to_ascii_lowercase()` 这一错误形式。
4. **新增 `PercentSegment`**：把 secret 字节序列压成两类段——
   - `Literal(Vec<u8>)`：一段连续的 ASCII 字母数字 secret 字节，**必须逐字节原样出现**（大小写敏感）；
   - `Escaped(u8)`：一个非字母数字 secret 字节，输入可以**字面写**或写成 `%XX`（hex 位大小写不敏感）。
   两类段都只含 ASCII，因此任何匹配的起止点必然落在输入的 UTF-8 字符边界上，
   可以直接切片拼接 `[REDACTED]` 而不需要解码。
5. **新增 `PercentForm`**：`segments` + `min_len`（全字面时的字节数）+ `max_len`（全转义时的字节数）。
   - `match_at(input, start) -> Option<usize>`：单趟左到右比对，命中返回**实际消耗字节数**
     （因为同一 secret 的匹配长度随转义与否变化，必须按实际值推进，不能用 `max_len`）。
   - `find_from(input, from) -> Option<(usize, usize)>`：先用 `anchors()`（首段的第一个字节；
     若首段是 `Escaped` 则为 `'%'` 与该字节）做 memchr 级锚点扫描，再对候选做完整比对。
   - `replace_in(input) -> String`：左到右、非重叠替换，语义与 `str::replace` 一致。
6. **`derived_forms()`**：返回类型改为 `Vec<RedactionForm>`，由 9 个形式变为 **8 个**
   （plain、hex 小写、hex 大写、percent 匹配器、base64、base64 无填充、base64url、base64url 无填充）。

### 4.2 复杂度与有界性（红线自查）

- **没有指数级变体枚举**：不生成 `%2d`／`%2D` 的组合。hex 大小写在**匹配期**用
  `hex_digit_value()` 逐位判定（`0-9/a-f/A-F`），复杂度恒定。
- **没有无限扫描**：`find_from` 的候选位置上限是 `input.len() - min_len`，
  `checked_sub` 不足即直接 `None`；每轮 `search` 严格单调递增（`candidate + 1`），
  命中后从命中终点继续，**不会重复扫描同一位置**。
- **总量**：`O(input_len × secret_len)` × 登记 secret 数 × 每 secret 的形式数（≤8），
  与原 `str::replace` 的朴素上界同阶；锚点扫描把常见情形压到接近 `O(input_len)`。
- **无新依赖**：只用 `std`（`slice::get`/`position`/`to_ascii_lowercase`）。`Cargo.toml`/`Cargo.lock` 未动。
- **可执行证据**：`scan_stays_bounded_on_long_adversarial_input` 用 20 万个 `A`
  （每个位置都是候选起点，即锚点扫描能遇到的最坏形状）断言原样返回，并在同一长串中间
  嵌入一个混合大小写 percent secret 断言仍被替换。整套 18 个用例含该用例 **0.07～0.10 s** 完成。

### 4.3 `crates/observability/tests/structured_logging.rs`（+239 / −0）

新增 `AUDIT_SECRET`（模拟）、`SharedBuffer::text()`、`audit_subscriber()`，以及 11 个新用例。
审计 RED 输入 3 个逐字迁入（只把字面量 `"[REDACTED]"` 换成既有常量 `REDACTED`，
断言与断言消息一字未改）：

| 类别 | 用例 |
|---|---|
| 审计 RED 迁入 | `allowed_plain_and_upper_hex_forms_are_sanitized`（合法对照）、`lower_hex_digits_keep_secret_letter_case`、`mixed_hex_digits_keep_secret_letter_case` |
| **实际 `JsonSubscriber` 字符串输出** | `structured_output_redacts_percent_encoded_secret_with_mixed_hex_case`：走真实 `tracing` 事件 → `JsonSubscriber` 写出，断言**原始 JSON 文本**里不含 `Audit`/`Only`/`AaB` 任何片段、出现 2 次 `[REDACTED]`、解析后 `fields.detail == "url=[REDACTED]&again=[REDACTED]"`，且 `request_id`/`team_id` 关联与 `message` 不受影响 |
| hex 大小写唯一归一 | `percent_hex_digit_case_is_the_only_normalization`（4 个固定形式，非组合枚举） |
| 混合 / 多段编码 | `partially_escaped_and_multi_segment_encodings_are_redacted` |
| UTF-8 | `utf8_secret_percent_forms_are_redacted`（`Ünïcode-Sécret`，含全小写 hex 与部分转义） |
| 边界 | `percent_boundary_conditions_do_not_over_match`（空串、`Audit%2`、`Audit%`、非法 hex 位 `%2Z`、`%2d%2f` 太短、缺尾字节、前缀命中后跟字符） |
| 未注册普通文本 | `unregistered_text_is_never_rewritten`（含 `Redactor::default()` 空策略） |
| 敏感字段 / plain·hex·base64 不变 | `sensitive_fields_and_plain_hex_base64_forms_keep_their_behaviour` |
| 有界性 | `scan_stays_bounded_on_long_adversarial_input` |

既有的 7 个用例（correlation、span 生命周期、敏感字段、`Sensitive` 表示）**一字未改，全部仍绿**。

## 5. 命令、exit code 与完整日志

全部日志在 `docs/reviews/evidence/minimax-return/MM-R04/`。

| # | 命令 | exit | 日志 |
|---|---|---:|---|
| 1 | `cargo test -p observability --locked --offline` | **0** | `test.log`（18 passed / 0 failed） |
| 2 | `cargo build -p observability --locked --offline` | **0** | `build.log` |
| 3 | `cargo clippy -p observability --all-targets --locked --offline -- -D warnings` | **0** | `clippy.log` |
| 4 | `RUSTDOCFLAGS="-D warnings" cargo doc -p observability --no-deps --locked --offline` | **0** | `rustdoc.log` |
| 5 | `rustfmt --edition 2024 [--check] <仅本包两个文件>` | **0** | `fmt.log`（前次差异留档 `fmt-check.log`，exit 1） |
| 6 | `cargo test --workspace --locked --offline`（越界自查，非验收要求） | 101 | `workspace-regression-check.log`，见 §8 |

第 5 项**只对 `crates/observability/src/lib.rs` 与 `crates/observability/tests/structured_logging.rs`
两个文件调用 `rustfmt`**，未执行 `cargo fmt --all`；`git status` 复核确认本包之外无新增改动。

## 6. 真实 RED → GREEN 证据

### 6.1 RED（修复前，src 未动）

- `red-1-migrated-audit-tests.log`：只把审计 3 个用例迁入 `structured_logging.rs` 后运行，
  **exit 101**，**8 passed** / **2 failed**。失败输出与审计 `observability-percent.log` 逐字一致：
  ```
  test lower_hex_digits_keep_secret_letter_case ... FAILED
    left: "Audit%2dOnly%2fAaB"   right: "[REDACTED]"
  test mixed_hex_digits_keep_secret_letter_case ... FAILED
    left: "Audit%2DOnly%2fAaB"   right: "[REDACTED]"
  ```
  合法对照 `allowed_plain_and_upper_hex_forms_are_sanitized ... ok`（同一轮里保持绿）。
- `red-2-full-new-suite.log`：补齐全部新用例后再跑一次，**exit 101**，10 passed / **7 failed**。
  （经 2026-10-08 复核：red-1 的「10」为用例总数而非通过数，正确值为 **8 passed / 2 failed**；
  red-2 的 10 passed / 7 failed 经核对无误。本订正不改变任何判定。）
  其中 `structured_output_redacts_percent_encoded_secret_with_mixed_hex_case` 的失败体是**真实
  结构化日志输出**，可直接看到泄露：
  ```json
  {"fields":{"detail":"url=Audit%2dOnly%2fAaB&again=[REDACTED]", ...}}
  ```
  同一行里大写形式 `Audit%2DOnly%2FAaB` 被脱敏、混合大小写形式没被脱敏——正是 DA-04 的形状。

### 6.2 GREEN（修复后）

`test.log`：18 passed / 0 failed，含两个原反例与合法对照。

### 6.3 反向验证（变异测试）——证明新断言不是恒真

`mutation-reversal.log`，每次只改 `src/lib.rs` 一处、跑完立即还原：

| 变异 | 期望 | 实测 |
|---|---|---|
| M1 `hex_digit_value` 去掉大写分支（退回只认 `%2d`） | 被杀 | **7 failed**，含两个原反例、合法对照、UTF-8、JsonSubscriber 输出用例 |
| M2 未转义字面段改成大小写不敏感（等价于旧实现"整体小写"的错误语义） | 被杀 | **第一次存活 17/17**，补反例后 **FAILED**（详见 §9 的自纠） |
| M3 转义段不再接受字面字节（退回严格 percent 编码） | 被杀 | **2 failed**：`partially_escaped_and_multi_segment_encodings_are_redacted`、`utf8_secret_percent_forms_are_redacted` |
| M4 去掉 UTF-8 字符边界守卫 `is_char_boundary` | — | **存活 17/17**，见 §9 剩余风险 |

## 7. 兼容性说明

**行为收紧（1 项，故意）**：删掉 `percent.to_ascii_lowercase()` 后，
secret `Audit-Only/AaB` 的"全小写 percent 串" `audit%2donly%2faab` **不再**被脱敏。
这是纠正而非退化：该串解码为 `audit-only/aab`，**不等于**已登记的 secret，
旧行为属于误伤式过度脱敏。`unregistered_text_is_never_rewritten` 在修复前就是红的
（`left: "[REDACTED]"` / `right: "audit%2donly%2faab"`），修复后转绿。

**行为放宽（1 项，derived reading，需 Codex 确认）**：非字母数字的 secret 字节现在
**可以字面出现、也可以写成 `%XX`**。这让 `Audit-Only%2FAaB`（部分转义）也被脱敏，
而修复前不脱敏。理由：这正是"这段文本 percent 解码后是否等于该 secret"的定义，
也对应任务目标里的"混合/多段编码"。冻结规格未逐字规定该粒度，故按 **derived reading** 登记，
若 Codex 判定应收紧，改动点是 `PercentForm::match_at` 里 `Escaped` 分支的首个 `if`
（删掉 3 行即可回到严格模式，`partially_escaped_and_multi_segment_encodings_are_redacted`
会随之转红，作为该语义的钉子）。

**逐字不变**：plain、hex（小写/大写）、base64/URL-base64（含/不含填充）、敏感字段名遮蔽、
`Sensitive<T>` 的 Display/Debug/Serialize、correlation 字段与 span 生命周期——
除 percent 匹配器外所有既有分支的代码路径未改，既有 7 个用例一字未动且仍绿。

**数据/协议/状态**：无 migration、无 wire 字段、无错误码、无领域状态、无权限语义变化。

## 8. 越界自查（超出验收要求，如实登记）

`cargo test --workspace --locked --offline` **exit 101**，唯一失败是
`persistence-sqlite` 的 `a_later_task_block_does_not_invalidate_historical_attention_resolution`
（`left: 25 / right: 24`，`crates/persistence-sqlite/tests/notification_execution_integrity.rs:470`）。

**与本包无关，已举证**：把 `crates/observability/src/lib.rs` 临时换回修复前的原始字节
（sha `9251f0ed…`）后单跑该用例**仍以完全相同的数字失败**（日志见
`workspace-regression-check.log` 末段）。且 `grep observability` 确认**工作区内没有任何 crate 依赖
observability**，本 crate 只被自己的测试使用。因此这是继承工作树里的既有失败
（属 DA-02／Notification 留存整改范围），本包不修、不掩盖，也不据此声称全工作区通过。

## 9. 自查中抓到并修正的问题

1. **M2 变异体第一次存活，暴露真实覆盖缺口**（不是"绿了就算过"）：
   我最初的"未注册文本不变"用例只放了首字母就不同的 `audit%2donly%2faab`，
   而锚点扫描本身要求首段首字节精确命中，于是**字面段大小写不敏感这个错误实现根本没被触发**。
   已补三个"首段命中、后续段不命中"的真反例（`Audit%2Donly%2fAaB`、`Audit-Only%2Faab`、
   `Audit%2dOnly%2faab`），补后 M2 被杀。
2. **M4 变异体存活**：`is_char_boundary` 守卫目前**不是承重件**。已给出证明：
   任何匹配的首段要么是 `Literal`（首字节必为 ASCII 字母数字），要么是 `Escaped`
   （首字节是 `%` 或 secret 的首字节，而 secret 是合法 UTF-8、首字节必为 ASCII 或前导字节），
   **都不可能是 UTF-8 续字节（0x80–0xBF）**，因此所有候选点天然落在字符边界上。
   保留该守卫是为了让"切片必在字符边界"成为显式不变量（供将来字面段纳入非 ASCII 时仍成立），
   但它今天的行为影响为零，**不得声称有测试覆盖它**。
3. 首次 clippy 报 3 条告警（`useless_conversion` ×2、`question_mark` ×1），均在本包新代码内，已修至 0 告警。

## 10. 未运行项及理由

| 项 | 理由 |
|---|---|
| canonical Mac 21 项门禁、干净安装、实机 Gate | ADR-0005 的正式门禁，由总控串行执行；本包离线且不应触碰机器状态 |
| TypeScript 层 `lint/typecheck/test/build/contracts:check` | 本包零 TS 变更、零生成物变更；且与本包写集无关 |
| `cargo audit` / 许可证 / 依赖门禁 | 本包零依赖变更（`Cargo.toml`/`Cargo.lock` 未动） |
| Gitleaks 全 refs / hosted CI | 无网络授权；本包未新增任何凭据字面量（模拟 secret 为常量） |
| 覆盖率（domain/application 90%/85% 门槛） | 属 Gate C 范围且审计已单列；本包为单 crate 缺陷修复，未新增测量装置 |
| 完整在线 Mac gate | 离线通过不等于完整在线门禁，本包不作此声明 |

## 11. 剩余风险

| # | 风险 | 等级 | 说明 |
|---|---|---|---|
| R1 | **纯 hex（全量十六进制）形式仍要求全小写或全大写**：整串 hex 的大小写混合形式（如 `77617665322D7365637265742d636f6f6b6965`）仍不会被脱敏 | 中 | 与 DA-04 同族但**不在本包范围内**（DA-04 明确是 percent）。本包未改动、未声称修复。若 Codex 判定应同批处理，需要另一次写集授权。 |
| R2 | **字母数字字节只按字面匹配**，不按 `%41`/`%61` 匹配：若某编码器把字母也转义，仍可绕过 | 中 | 属冻结规格未定义的粒度，本包按"派生形式只转义非字母数字"的既有口径保持不变（derived reading，已登记）。 |
| R3 | base64/base64url 派生形式未动 | 低 | DA-04 未涉及；行为与修复前一致。 |
| R4 | `is_char_boundary` 今日不可测（见 §9.2） | 低 | 有证明其为不可达分支，非承重件。 |
| R5 | 派生形式只覆盖**完整** secret；若日志里只出现 secret 的真子串且被切分，仍不可见 | 低 | 与修复前一致，非本包引入。 |
| R6 | 性能：`O(input_len × secret_len)` × secret 数 × ≤8 形式；长文本+多 secret 的最坏情形未做基准测量，仅有用例级有界性断言 | 低 | 无墙钟断言（避免脆弱测试）；`scan_stays_bounded_on_long_adversarial_input` 提供有界性证据。 |

## 12. 回退方式

1. **单文件回退**：`cp docs/reviews/evidence/deep-audit-2026-10-08/main-repro/crates/observability/src/lib.rs crates/observability/src/lib.rs`
   （该副本与修复前逐字节相同，sha `9251f0ed…`），再 `rustfmt --check` 并跑
   `cargo test -p observability`；此时本包新增的 11 个用例会转红 7 个，这是预期的回退特征。
2. **测试回退**：`crates/observability/tests/structured_logging.rs` 删除
   `AUDIT_SECRET` / `text()` / `audit_subscriber()` 与第 4 节表格所列 11 个用例即可，
   既有 7 个用例未改。
3. 无 migration、无锁文件、无生成物、无外部状态需要回滚；本包未提交/推送/部署，
   回退不需要联系任何外部系统。

## 13. 自验收结论

**`SELF_CHECK_PASS`**

判定依据：审计两个反例真红→真绿、合法对照全程保持绿；已补实际 `JsonSubscriber` 字符串输出断言；
混合编码／UTF-8／边界／未注册文本／敏感字段／plain·hex·base64／correlation 均有行为断言；
三个语义变异体被杀、M4 存活已如实登记；四条验收命令 exit 全 0；零新依赖、写集未越界、
未提交未推送未部署、未使用真实 secret。

**自验收不等于 DONE。** DEV-01-08 仍为 `BLOCKED`，DA-04 是否关闭、
§7 的 derived reading 是否接受、§11 的 R1/R2 是否同批处理，均待 Codex 独立逐差异复核裁定。
本端不解除任何 BLOCKED、不启动依赖节点。