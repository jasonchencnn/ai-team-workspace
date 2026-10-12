# FC-04｜一次 percent 编码下字母数字字节漏检（MR-02）回传

关联节点 **DEV-01-08**（当前 `BLOCKED`，本包不解除）。缺陷编号 **MR-02**，P1 凭据泄露，R3。
本文件只是执行端自验收回传，**不等于 DONE**；节点状态、缺陷关闭与任何待决项均待 Codex 独立复核裁定。

---

## 1. 实际执行端与环境

| 项 | 值 |
|---|---|
| Harness | MiniMax Code（本地 runtime，macOS darwin 27.0.0 arm64） |
| 实际模型/版本 | `MiniMax-M3.1-Flash-Preview`（执行端回填，不假定为 M3） |
| 仓库 | `/Users/chenzhiqing/Documents/MyVault/协作工作` |
| HEAD / 分支 | `1a14b8795e76fc373bef326eb5df96f9ac173f71` / `codex/wave-2-ci-validation` |
| 共享工作树 | 与其他包共用未提交工作树；**全部继承成果保留**，未执行 `reset`/`checkout`/`clean`/`stash` |
| 工具链 | cargo 1.98.0 / rustc 1.98.0 / rustfmt 1.9.0-stable / clippy 0.1.98（`PATH=/Users/chenzhiqing/.minimax/toolchain/bin`） |
| 网络 | 全部命令 `--locked --offline`，`CARGO_NET_OFFLINE=true`；无下载、无外部服务 |
| 依赖 | `crates/observability/Cargo.toml`、`Cargo.toml`、`Cargo.lock` **一字未改**；零新增依赖 |
| 新增费用 | $0；未提交/推送/部署；未访问外部账号 |
| 只用模拟 secret | 全程只用模拟串 `Audit-Only/AaB`、`wave2-secret-cookie`、`Ünïcode-Sécret` 等；未读取、写入或打印任何真实凭据 |

## 2. 输入与输出 SHA

| 文件 | 输入 SHA（本包开始时） | 输出 SHA（本包结束时） |
|---|---|---|
| `crates/observability/src/lib.rs` | `fc1d5bf801c37be327d434655c9be2874b1b2854683222098b419a1b268e7586` | `d945dd5bd98c2d137a4a2e5c92583fbddba5668463788957e26b957f1eb4b756` |
| `crates/observability/tests/structured_logging.rs` | `2bed8f18ac9438f3be9e8a5f59c0d1d98a3b00ba740a358a77394545911c8217` | `f843839d48297857d6664fb19260b59a7d253a09690577485eef1dbe92463e76` |

- 修复前字节另存为 `baseline/lib.rs.pre-fix`（SHA 同上 `fc1d5bf8…`），供 RED 回放与回退使用。
- 本包的证据目录、结果文件为本包写集；未改任何其他文件（`git status` 中其他 `M/??` 项均为本包开始前即存在的继承成果或并发包 FC-01/02/03/05 的改动）。

## 3. 缺陷事实与本包实现的语义

修复前 `PercentForm::new` 把 secret 的**字母数字字节固定编成 `PercentSegment::Literal`**，
`match_at` 对 `Literal` 段只接受**字节精确相等**。因此一次 percent 编码下，只要编码器把
**任意一个字母数字字节**写成 `%XX`（`%41`、`%4f`、`%42`…），整条模式就匹配失败，secret 原样进日志。

本包实现的语义（三条，均已冻结在测试里）：

1. **一次 percent 编码下，每个注册字节都可写成字面或 `%XX`**（不再只允许非字母数字字节转义）。
2. **`%XX` 的两个 hex 位大小写不敏感**（`%2d` ≡ `%2D`）。
3. **解码出的字节保持注册大小写**：字面字节必须与注册字节**完全相等**，`Audit` ≠ `audit`，
   `audit%2donly%2faab` 保持原文（沿用上轮已接受的边界裁决）。
4. **纯 hex 形式逐位比较**，任意大小写混合等同全小/全大写；窗口不等于注册拼写时逐字节保持不变。

**明确不实现**：递归／多层 percent 解码。`%25` 在这里只是一个普通字节，深度与误伤策略仍待决。

## 4. 逐文件修改

写集严格限于任务包所列：`crates/observability/src/lib.rs`、
`crates/observability/tests/structured_logging.rs`，外加本 RESULT 与本包 evidence 目录。

### 4.1 `crates/observability/src/lib.rs`（+289 / −32 相对 HEAD，其中本包净改集中于匹配器）

1. **删除 `PercentSegment`（`Literal(Vec<u8>)` / `Escaped(u8)`）**，改为**逐字节规则**：
   `PercentForm { bytes, min_len, max_len, anchors }`。`match_at` 对每个注册字节：
   - 输入字节与注册字节相等 → 消耗 1 字节（大小写敏感）；
   - 否则输入必须是 `%XY`，`hex_digit_value` 逐位取值后等于该注册字节 → 消耗 3 字节。
   两者都失败即 `None`。返回**实际消耗的字节数**（拼接只用这个值，不用 `max_len`）。
2. **新增 `HexForm { digits, anchors }`**：`digits` 是每个注册字节的高/低半字节序列（长度 `2n`）；
   `match_at` 用 `hex_digit_value(actual) == Some(expected)` 逐位比较，大小写天然无关；
   `anchors` 由 `hex_digit_spellings` 给出首个半字节的一个或两个字节拼写。
3. **新增共享有界扫描** `replace_bounded_matches` / `find_bounded_match`，
   供两个匹配器共用；`RedactionForm` 新增 `Hex(HexForm)` 变体，`sanitize` 相应分派。
4. **`derived_forms`**：由 8 个形式改为 **7 个**（percent、hex、plain、base64 标准/URL × 有填充/无填充）。
   旧的 `Exact(hex)` 与 `Exact(hex.to_ascii_uppercase())` 被 `Hex(HexForm)` 取代（后者覆盖两者并额外覆盖任意大小写混合）。
   空 secret 仍在函数开头整体早退（`PercentForm::new`/`HexForm::new` 返回 `None`），
   避免 `Exact("")` 那种会改写每一个位置的灾难形态。
5. `Redactor` 的文档注释重写为冻结语义 + 有界性说明；`hex_digit_spellings` 为新增小工具。
   `is_sensitive_field`、`Sensitive<T>`、correlation、span 生命周期、base64 编码全部未动。

### 4.2 匹配算法如何保证有界且不枚举变体（重点）

- **不枚举变体**：一次 percent 编码的 `2^n` 种拼写、`2^(2n)` 种 hex 大小写拼写**一个都不生成**。
  变体判定发生在**匹配期**：percent 用 `hex_digit_value` 逐位比较（`0-9/a-f/A-F`），
  hex 用 `digits[i]` 与 `hex_digit_value(input[j])` 比较。两者都是 O(1)/位。
- **不做全串大小写折叠**：实现里没有任何 `to_ascii_lowercase()` 作用于输入或输出；
  输入字节只被读取和比较，非 secret 文本逐字节保持不变（M5 变异专门验证这一点会被杀）。
- **锚点预筛**：候选起点只有两类字节——注册的**首字节**与 `%`（hex 是首个半字节的 1～2 种拼写）。
  因此每个位置的候选密度有上界，且与 secret 长度无关。
- **候选上界**：`last_start = input.len().checked_sub(min_len)?`；不足即整体 `None`。
  每轮失败后 `search = candidate + 1`，命中后 `search = 命中终点`，**游标单调不回退**，
  不存在从头重扫，因此每个输入位置被检查的次数是常数级。
- **单趟比对**：`match_at` 每个注册字节最多走一次分支，代价 `O(secret_len)`。
  合计 `O(input_len × secret_len)` × 注册 secret 数 × 每 secret 形式数（7），与修复前的朴素上界同阶，
  且实测 ns/byte 在 64 倍尺寸区间内恒定（见 §8）。
- **终止性**：`length ≥ 1`（注册字节非空），`search` 严格递增；`match_at` 只在 `input.get()` 成功时前进，
  故 `start + length ≤ input.len()`，切片永不越界、不 panic。
- **零新依赖**：只用 `std`（`slice::get`、`iter().position`、`then_some`）。

### 4.3 `crates/observability/tests/structured_logging.rs`（+651 行；11 个新用例）

既有 18 个用例的**断言与文案一字未改且全部仍绿**；唯一的既有改动是把 `subscriber` /
`audit_subscriber` 两个 helper 改为委托新的 `fixed_timestamp_subscriber`（时间戳与 redactor 参数完全相同，行为不变）。

| 类别 | 用例 | 覆盖要求 |
|---|---|---|
| **主控真实路径四种漏检**（走真实 `JsonSubscriber` 字符串输出） | `every_registered_byte_may_be_literal_or_escaped_in_structured_output` | `%41`首字节、`%4f`、`%41`(a)、`%42`(B) 四种 + 全字节 `%XX`，前后普通文本原样保留 |
| 正对照 | `plain_and_symbol_escaped_spellings_stay_redacted_as_the_positive_control` | 修复前后都应绿，证明不是"全红" |
| `redact_error` / `redact_event` | `error_and_event_paths_escape_every_registered_byte` | 两个公开错误/Event 入口对五种拼写一致 |
| 纯 hex 任意大小写混合 | `pure_hex_spelling_treats_every_digit_case_mix_as_one_form` | 全小/全大/逐位混合等价；末位改一位、少一个字节对、多一个字节对、错位、无关 hex 逐字节不变 |
| 前后普通文本 / 重复 / 多 segment | `surrounding_plain_text_and_repeats_are_never_rewritten` | URL 前缀、`AUDIT_TOKEN=` 大写上下文、后缀、相邻重复、hex 段 |
| 畸形/截断 | `malformed_and_truncated_encodings_are_left_untouched` | `%`、`%2`、`%4`、`%zz`、`%41u`、`Audit%`、`Audit%2`、`%2d%2f`、半转义、完整匹配后跟截断 `%4` |
| 空 secret | `an_empty_registration_redacts_nothing_and_still_honours_real_secrets` | `[""]`、空迭代、`default()` 均不改写；与真实 secret 混登仍生效 |
| 多个 secret 重叠 | `overlapping_registered_secrets_are_all_removed` | `Audit-Only/AaB` / `Audit-Only/Aa` / `Only/AaB` / `AaB` 前缀重叠，最长形式优先 |
| UTF-8 | `utf8_secret_accepts_full_escaping_and_keeps_registered_case` | 全字节转义、大小写混合 hex、纯 hex 三种大小写、`值=…。` 上下文、同形不同大小写保持原文 |
| sensitive 字段 | `sensitive_field_names_stay_masked_next_to_matching_content` | `authorization` 按字段名遮蔽 + 同值在普通字段按内容遮蔽 |
| 有界扫描 | `percent_scan_is_bounded_on_escaped_and_near_miss_inputs` | 近失配（每个候选对齐 12/13 字节）、纯转义流、长串尾部仍能命中全转义拼写 |

**泄露断言口径**：所有新用例都调用 `assert_rendered_hides_secret`，它对渲染结果做**一次 percent 解码**，
断言解码后不再含注册 secret（另有明文断言）。这样断言探测的是"危害是否发生"，而不是"某个实现细节是否被改动"，
不会因无关改写而假绿。

## 5. 命令、exit code 与完整日志

全部日志在 `docs/reviews/evidence/minimax-followup/FC-04/`。

| # | 命令 | exit | 日志 |
|---|---|---:|---|
| 0 | `cargo test -p observability --locked --offline`（改前基线） | 0 | `00-baseline-test.log`（18 passed / 0 failed） |
| 1 | `cargo test -p observability --locked --offline --no-fail-fast`（RED，共享 src 未动） | **101** | `01-red-tests-vs-unmodified-src.log`（22 passed / **7 failed**，测试版本 SHA `e0340a7a…`，见 §6 说明） |
| 2 | `cargo test -p observability --locked --offline`（中间 GREEN 迭代） | 101→0 | `02-green-test.log`（保留第一次 GREEN 前的失败，用于说明期望值自身被修正） |
| 3 | `rustfmt --edition 2024 --check <本包两个文件>`（格式化前） | 1 | `03-fmt-check-pre.log` |
| 4 | `cargo test -p observability --locked --offline`（终态） | **0** | `04-test-final.log`（**29 passed / 0 failed**） |
| 5 | `cargo build -p observability --locked --offline` | **0** | `05-build.log` |
| 6 | `cargo clippy -p observability --all-targets --locked --offline -- -D warnings` | **0** | `06-clippy.log`（首次有 2 条本包测试内告警，已修，见 §9） |
| 7 | `RUSTDOCFLAGS="-D warnings" cargo doc -p observability --no-deps --locked --offline` | **0** | `07-rustdoc.log` |
| 8 | `rustfmt --edition 2024 --check <本包两个文件>`（终态） | **0** | `08-fmt-check.log` |
| 9 | 沙箱 RED：`cargo test --locked --offline --no-fail-fast`（**修复前 src `fc1d5bf8…` + 终态测试 `f843839d…`**） | **101** | `09-sandbox-red-pre-fix-src.log`（21 passed / **8 failed**） |
| 10 | 沙箱 GREEN：同一沙箱（**修复后 src `d945dd5b…` + 同一份测试 `f843839d…`**） | **0** | `10-sandbox-green-fixed-src.log`（29 / 0） |
| 11 | 变异台账 `python3 run-mutations.py <repo>` | 见 §7 | `11-mutation-results.log` |
| 12 | 源码副本完整性核对（共享 / 沙箱 / pristine / pre-fix / 测试 五方 SHA） | 0 | `12-source-copy-restore.log` |
| 13 | 资源上界探针（debug） | **0** | `13-resource-bound-debug.log` |
| 14 | 资源上界探针（release） | **0** | `14-resource-bound-release.log` |

第 3/8 项**只对本包两个文件**调用 `rustfmt`，**未执行 `cargo fmt --all`**。

## 6. 真实 RED → GREEN 与正对照

**RED（权威，`09`）**：在**独立源码副本**里用修复前 src（SHA `fc1d5bf8…`）+ **终态测试文件**
（SHA `f843839d…`）跑完整套件 → **21 passed / 8 failed，exit 101**。八条失败全部是审计记录的泄露形态，例如：

```
every_registered_byte_may_be_literal_or_escaped_in_structured_output
  left: "prefix url=%41udit%2dOnly%2fAaB suffix"
 right: "prefix url=[REDACTED] suffix"
pure_hex_spelling_treats_every_digit_case_mix_as_one_form
  case: digit-by-digit mix
  left: "blob=0x41756469742D4F6e6c792f416142::ready"
 right: "blob=0x[REDACTED]::ready"
utf8_secret_accepts_full_escaping_and_keeps_registered_case
  case: hex digit-by-digit mix
  left: "c39c6EC3AF636f64652d53c3a963726574"
 right: "[REDACTED]"
surrounding_plain_text_and_repeats_are_never_rewritten
  left: "GET /v1/task?AUDIT_TOKEN=%41udit%2dOnly%2fAaB&retries=3|[REDACTED]|…"
```

**GREEN（`10`）**：**同一个沙箱**、**同一份测试字节**，只把 src 换成修复后版本（SHA `d945dd5b…`）
→ **29 passed / 0 failed，exit 0**。RED 与 GREEN 的唯一差异就是 src，因此差异只可能来自修复。

**另有一份 RED（`01`）是在共享 crate 上跑的**，共享 src 全程未被改动（这是允许的：不允许的是改共享 src，
而当时的 src 正是修复前字节）。它的测试版本是 `e0340a7a…`（早于最后两处测试加强），
结果 22 passed / 7 failed、exit 101，泄露形态相同。**最终以 `09`/`10` 这一对为准**，
`01` 保留作为"在真实 crate 上、修复前 src 的真实行为"的旁证。

**正对照在 RED 同一轮里保持绿**（证明不是"全红式"失败）：
`plain_and_symbol_escaped_spellings_stay_redacted_as_the_positive_control`、
`malformed_and_truncated_encodings_are_left_untouched`、
上轮 11 个用例（含 `structured_output_redacts_percent_encoded_secret_with_mixed_hex_case`、
`unregistered_text_is_never_rewritten`、`percent_boundary_conditions_do_not_over_match`）全部 `ok`。

## 7. 变异证据

装置：`run-mutations.py`（随证据归档，可重放）。每次只在
`source-copy/crates/observability/src/lib.rs` 上打一处补丁，跑完整套件，**立即从 `pristine-lib.rs` 还原**，
并核对还原后的 SHA。共享 `crates/observability/src/lib.rs` 全程未被改动
（`12-source-copy-restore.log` 记录沙箱与共享文件 SHA 一致）。

| 变异 | 含义 | 结果 | 杀死它的用例数 |
|---|---|---|---:|
| M1 | 只接受 `%XX`，字面字节一律不匹配 | **KILLED** | 18 |
| M2 | 字面字节改成大小写不敏感 | **KILLED** | 3 |
| M3 | 去掉 `%` 锚点 | **KILLED** | 8 |
| M4 | hex 窗口拒绝大写数字位 | **KILLED** | 3 |
| M5 | **匹配前把整串折成小写**（被明令禁止的取巧做法） | **KILLED** | 22 |
| M6 | 用 `max_len` 而不是实际消耗长度推进 | **KILLED** | 21 |
| M7 | hex 锚点只给小写字母拼写 | **KILLED** | 1 |
| M8 | 去掉 UTF-8 字符边界守卫 `is_char_boundary` | **存活（0）** | 0 |

- 每条变异的 mutated / restored SHA 见 `11-mutation-results.log`，全部 `restored == pristine == d945dd5b…`。
- **M8 存活的原因已证明，不是覆盖缺口**：候选起点只有 `%`、十六进制数字（均为 ASCII）与
  **注册 secret 的首字节**。合法 UTF-8 的首字节要么是 ASCII，要么是前导字节（0xC2–0xF4），
  **不可能是续字节 0x80–0xBF**，故所有候选点天然落在字符边界上。该守卫是显式不变量，
  保留它是为了让"切片必在字符边界"在将来规则放宽时仍然成立，**不得声称今天有测试覆盖它**。
- M7 第一次存活是因为沙箱里的测试副本是旧字节；修正装置（每次先从共享测试文件同步）后 M7 被
  `utf8_secret_accepts_full_escaping_and_keeps_registered_case` 杀死——该用例特意选了一个
  hex 拼写以字母开头（首字节 0xC3 → 半字节 `c`）的 UTF-8 secret，正是为覆盖这条路径。

## 8. 资源上界证据

`probes/fc04_bound_probe.rs`（只存在于证据副本，不进生产写集）。所有形状同时**断言结果**，
不只计时。取 release 数字（`14-resource-bound-release.log`，debug 见 `13`）：

| 形状 | 输入字节 | keys | ms | ns/byte |
|---|---:|---:|---:|---:|
| near_miss（每 13 字节一个候选，各自对齐 12/13 后失败） | 57 344 → 3 670 016（64×） | 1 | 0.234 → 14.216 | 4.1 → 3.9 |
| escapes（每 3 字节一个 `%` 锚点） | 12 288 → 786 432 | 1 | 0.082 → 3.548 | 6.7 → 4.5 |
| anchors（每个字节都是锚点） | 4 096 → 262 144 | 1 | 0.028 → 1.458 | 6.7 → 5.6 |
| hex_digits（每 10 字节一个 hex 锚点） | 40 960 → 2 621 440 | 1 | 0.195 → 13.657 | 4.7 → 5.2 |
| ordinary（无任何锚点） | 208 896 → 13 369 344 | 1 | 6.031 → 351.270 | 28.9 → 26.3 |
| escapes（8 个 secret，56 个形式） | 12 288 → 786 432 | 8 | 0.535 → 32.387 | 43.6 → 41.2 |
| embedded（长串中夹全转义拼写，须被移除） | 16 426 → 1 048 618 | 8 | 0.508 → 37.595 | 30.9 → 35.9 |

- **ns/byte 在 64 倍尺寸跨度内基本恒定**（1 key 约 4～7 ns/byte，8 keys 约 30～45 ns/byte），
  即代价随输入线性、与任何拼写数量无关；若实现枚举变体或回头重扫，ns/byte 会随规模上升。
- 单次上界断言（同一探针内，5 秒的宽松天花板，实测远低于此）：
  1 400 000 字节 near_miss + 1 key = **6.55 ms**；同输入 + 8 keys = **31.60 ms**；
  1 200 000 字节 escapes + 8 keys = **48.95 ms**（debug 下分别 94.8 / 320.7 / 1679.7 ms）。
- 生产形态通常只有少量短字符串；上表给出的是**每 secret 每形式都命中锚点**的最坏形状。

## 9. 自查中抓到并修正的问题（如实登记）

1. **测试期望值自身错误两处**（不是产品缺陷）：先把 `AUDIT_SECRET_HEX_LOWER` 写错成
   `416975642d…`（正确是 `41756469742d4f6e6c792f416142`），使两条 hex 用例**因错误原因而红**；
   是沙箱探针 `probes/fc04_probe.rs` 打出来的。修复方式不是改断言，而是**让所有大小写/近失配样本
   由常量派生**，杜绝手抄错误。修正后 hex 用例只因"逐位混合大小写"这一真实缺口而红。
2. **原始 JSON 文本没有前导空格**：`raw.contains(" x ")` 恒假（字段值紧跟引号），
   已改为对解析后的 `detail` 与 `[REDACTED]` 计数断言。
3. **"包含完整拼写的更长文本"必须被脱敏**：`hex + "42"`、`"14" + hex` 会正确脱敏，
   我最初的"应保持原文"期望是错的，已按与既有 `Audit-Only/AaBX → [REDACTED]X` 一致的口径改正。
4. **探针形状与注册 secret 冲突**：`near_miss` 形状含另一个已注册 secret `Audit-Only/Aa`，
   8-key 场景下被正确脱敏；探针已改为 1-key 用该形状、8-key 用 `escapes` 形状。
5. **clippy 首次报 2 条本包测试内告警**（`op_ref` / `needless_borrow`），已修至 0 告警。
6. **我自己写的两份证据日志口径不一致，已重跑修正**：① `01` 用的测试版本早于最后两处测试加强，
   已在 §6 明确区分，最终以沙箱 `09`/`10` 为准；② `09` 的首行曾打印共享文件的 SHA 而非被测沙箱文件的 SHA，
   已重跑，被测文件 SHA 现直接写进日志（`fc1d5bf8…`）；③ 重跑时发现资源探针仍留在沙箱 `tests/` 里污染对照，
   已移出（探针单独对修复后 src 跑，见 §8）。

## 10. 明确未声称

- **未实现递归／多层 percent 解码**。`%2541udit…`（两轮编码）仍不在覆盖内；深度与误伤策略未冻结，按裁定列为待决。
- **未验证真实凭据场景**：全部断言只用模拟 secret；未接触任何真实 token/cookie/keychain 数据。
- **未做分支覆盖率测量**：未新增测量装置，未声称 branch 覆盖达标。
- **未运行全工作区 Rust 测试/门禁**：其他包（FC-01/02/03/05）在同一工作树并发修改，
  其失败不能归因本包；已核实**工作区内没有任何 crate 依赖 observability**（`grep` 证据见 §2 说明），
  本包改动不可能改变其他 crate 的行为。
- **未做联网 audit / 许可证 / hosted CI / Gitleaks 全 refs 扫描**：无网络授权，且本包零依赖变更。
- **不声称 MR-02 已关闭、DEV-01-08 已 DONE、待决的递归解码策略已定**。

## 11. 兼容性说明

| 类别 | 内容 |
|---|---|
| 行为收紧（有意） | ① 一次 percent 编码下**任何**注册字节都可写成 `%XX`（此前仅非字母数字字节可以）；② 纯 hex 的任意大小写混合被脱敏（此前只有全小/全大）。两者都符合"该段文本经一次 percent/hex 解码后等于注册 secret"的定义，符合防泄露意图。 |
| 行为不变 | 明文、base64 标准/URL-safe（含/不含填充）、敏感字段名遮蔽、`Sensitive<T>` 的 Display/Debug/Serialize、correlation 字段、span 生命周期、form 排序口径（按最长匹配字节长度倒序，长形式优先）。 |
| 上轮已接受、本包保持 | `audit%2donly%2faab` 解码不是已注册 secret → 保持原文，**正确**。 |
| 数据/协议/状态 | 无 migration、无 wire 字段、无错误码、无领域状态、无权限语义变化。 |

## 12. 未运行项及理由

| 项 | 理由 |
|---|---|
| 全工作区 `cargo test` / Mac 21 项门禁 | 其他包在同一共享工作树并发修改，结果不可归因；observability 无下游依赖 |
| `cargo audit` / 许可证 / 依赖门禁 | 本包零依赖变更 |
| 分支覆盖率 | 本包未新增测量装置，审计已单列；不作声称 |
| Gitleaks / hosted CI | 无网络授权；本包未新增任何凭据字面量（模拟 secret 为常量） |
| 真实凭据 / 生产日志回放 | 无授权，且规范禁止 |

## 13. 剩余风险

| # | 风险 | 等级 | 说明 |
|---|---|---|---|
| R1 | **多层 percent 解码仍未覆盖**：`%2541udit…` 两轮编码仍可绕过 | 中 | Codex 已裁定为待决项，本包**刻意未实现**；需要冻结深度与误伤策略后另签写集 |
| R2 | **匹配面放宽后的误伤**：任何"一次解码等于注册 secret"的文本都会被脱敏，可能覆盖普通日志（例如 secret 很短时） | 低-中 | 这是防泄露意图的直接结果；若某天需要更细粒度，须改用结构化字段而非文本匹配 |
| R3 | **纯 hex 只覆盖完整 secret 的完整 hex 串**，不覆盖 hex 的子串/大小写无关的 base16 变体（如以 `0x` 前缀或分隔符书写） | 低 | 与修复前同族，非本包引入 |
| R4 | **base64/base64url 派生形式未动**，同样只覆盖全小写规范拼写 | 低 | DA-04/MR-02 未涉及，行为与修复前一致 |
| R5 | `is_char_boundary` 守卫今天不可测（M8 存活） | 低 | 已给出证明其为不可达分支，非承重件；不得声称有覆盖 |
| R6 | 派生形式只覆盖**完整** secret；真子串被切分后仍不可见 | 低 | 与修复前一致 |
| R7 | 成本上界是 `O(input_len × secret_len) × secrets × forms`；8 secret × 1.2 MB 最坏形状 release 约 49 ms | 低 | 已实测并留证；若将来 secret 数量或日志体积显著上升，需重新评估 |

## 14. 回退方式

1. **单文件回退**：`cp docs/reviews/evidence/minimax-followup/FC-04/baseline/lib.rs.pre-fix crates/observability/src/lib.rs`
   （与本包开始时的字节完全相同，SHA `fc1d5bf8…`），再 `rustfmt --edition 2024 --check` 该文件并跑
   `cargo test -p observability`。此时本包新增的 11 个用例会转红 7 个 —— 这是预期的回退特征。
2. **测试回退**：删除本包在 `crates/observability/tests/structured_logging.rs` 中新增的
   `ENCODED_MISSES` / `AUDIT_SECRET_HEX_LOWER` / `hex_digit` / `percent_decode_once` /
   `assert_rendered_hides_secret` / `log_error_detail` / `fixed_timestamp_subscriber` 与 §4.3 表中 11 个用例；
   上轮 18 个用例与 helper 语义不变（`subscriber` / `audit_subscriber` 改为委托 `fixed_timestamp_subscriber`，行为一致）。
3. 无 migration、无 lockfile、无生成物、无外部状态需要回滚；本包未提交/推送/部署，
   回退不需要联系任何外部系统。

## 15. 自验收结论

**`SELF_CHECK_PASS`**

判定依据：审计记录的四种漏检与全字节转义在**真实 `JsonSubscriber` 字符串输出**上真实 RED→GREEN，
`redact_error` / `redact_event` 同覆盖；纯 hex 任意大小写混合、UTF-8、前后普通文本、重复/多 segment、
畸形/截断、空 secret、多个 secret 重叠、sensitive 字段全部有行为断言；正对照在同一轮保持绿；
八个语义变异中 7 个被杀、1 个存活且存活原因有证明；资源上界给出可重放的线性实测；
五条验收命令 exit 全 0；零新依赖、写集未越界、未提交未推送未部署、未使用真实 secret。

**自验收不等于 DONE。** DEV-01-08 仍为 `BLOCKED`；MR-02 是否关闭、R1（多层解码）是否另签写集、
R2 匹配面放宽是否接受，均待 Codex 独立反例审查与最终准入裁定。本端不解除任何 BLOCKED、不启动依赖节点。