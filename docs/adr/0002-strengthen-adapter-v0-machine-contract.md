# ADR-0002：补强 Adapter V0 机器契约

- 状态：Accepted
- 日期：2026-09-22
- 决策者：Human Owner（2026-09-22 明确确认）
- 适用：DEV-03-01 Adapter V0 JSON Schema、契约测试与 Wave 1 复核

## 背景

Adapter V0 的冻结文字规格已经要求协议 ID 使用规范小写 UUIDv7、协议时间使用 UTC RFC 3339 毫秒格式、`run.output` 受内容与消息大小限制、自定义 JSON-RPC 错误映射到冻结码表且携带统一 `error.data`，版本使用 SemVer。

Wave 1 独立协议专项审计发现，已发布的 JSON Schema 没有完整执行这些约束：通用 UUID/日期格式会接受不符合冻结表示的值，`run.output` 可携带空或错配载荷，JSON Schema 的 `maxLength` 不能表达 UTF-8 字节预算，自定义错误范围与数据不完整，SemVer 模式允许前导零。详见 `docs/reviews/WAVE-1-INDEPENDENT-PROTOCOL-AUDIT.md`。

这些是机器契约对既有冻结语义的漏实现，不是产品或协议语义的新需求。Human 已授权按专项审计路线整改，不启动 Wave 2。

## 决策

1. Adapter V0 的所有领域/追踪 UUID 字段继续使用 UUIDv7，并由 Schema 同时强制 UUID 格式、版本位、variant 位、连字符和小写规范表示。JSON-RPC `id` 保持标准允许的字符串或非负整数，不冒充领域 UUID。
2. 所有协议时间继续使用 UTC RFC 3339，并由 Schema 同时执行有效日期校验和固定 `YYYY-MM-DDTHH:mm:ss.sssZ` 表示。
3. `run.output` 按 `kind` 建立互斥载荷：`stdout`、`stderr`、`progress` 必须且只能携带非空 `content`；`artifact_candidate` 必须且只能携带 `kind=artifact` 的 Artifact reference。禁止 `null` 和空载荷。
4. `content` 的 64 KiB 上限按 UTF-8 字节计算；整条 JSON Lines 消息的 1 MiB 上限按 UTF-8 字节计算。Schema 使用 `x-maxUtf8Bytes` 与 `x-maxJsonUtf8Bytes` 标注机器预算，协议 validator 必须实现这两个关键字或在进入 Schema 校验前执行等价字节检查。传输层必须在解析前检查原始行的 1 MiB 上限，不能只依赖字符数或规范化后重新序列化的长度。
5. JSON-RPC 标准错误 `-32700`、`-32600`、`-32601`、`-32602`、`-32603` 保持独立兼容分支，允许标准可选 `data`。平台自定义错误仅允许冻结的 `-32010`～`-32018` 与 `-32099` 离散集合，必须携带统一 `error.data`，并强制 JSON-RPC code、`domain_code` 与 `retryable` 的冻结映射。
6. 协议与 Adapter 版本使用规范 SemVer 2.0.0 表示：核心版本和纯数字预发布标识不得有前导零；允许规范预发布和构建元数据。
7. 协议版本仍为 `0.1.0`。本 ADR 只让机器契约拒绝文字规格从未允许的值，没有改变符合冻结协议的合法 payload、方法、字段或业务能力。
8. 专项审计中的每个反例进入负向契约测试；全部既有示例和兼容正例继续通过。未通过全量 Wave 1 门禁时，DEV-03-01 与 Wave 1 保持 `BLOCKED`。

## 兼容影响

- 已符合冻结 Adapter V0 规格的实现不需要改变 payload，现有正向示例保持有效。
- 曾依赖宽松 Schema 发送 UUIDv4/大写 UUID、非 UTC 或非三位毫秒时间、空/错配 output、超字节预算、未冻结自定义错误、缺失统一错误数据或非规范 SemVer 的实现会被拒绝。这是把既有非法输入从下游拒绝前移到协议边界。
- 标准 JSON-RPC 错误不要求自定义 `error.data`，因此与 JSON-RPC 2.0 的标准错误行为保持兼容。
- Schema 新增的 `x-*` 字节关键字需要 validator 显式实现；不能识别关键字的 validator 不具备 DEV-03-01 合规性。

## 回退方式

如果补强导致符合原冻结文字规格的 payload 被误拒绝：

1. 停止 Wave 2，保持 DEV-03-01/Wave 1 为 `BLOCKED`；
2. 回退本 ADR 对应的 Schema、契约测试和协议文档变更，不修改业务数据或执行迁移；
3. 用最小复现 payload 修正约束，重新进行 Human 复核；
4. 重跑本 ADR 的全部验证标准，通过后才能恢复 `DONE/PASS`。

回退不允许临时放宽为“先接收、后校验”，也不允许删除专项审计反例来恢复绿色。

## 验证标准

1. 现有 manifest/message 示例与标准 JSON-RPC 错误兼容用例全部通过。
2. UUIDv4、大写 UUID、带时区偏移、无毫秒、六位微秒、非法日期全部被拒绝。
3. `run.output` 的 `null`、空字符串、空 Artifact、kind/载荷错配被拒绝；Artifact candidate 只接受 Artifact reference。
4. UTF-8 `content` 恰好 64 KiB 通过、超过 1 byte 被拒绝；完整消息恰好 1 MiB 通过、超过 1 byte 被拒绝。
5. 缺少 `error.data`、未冻结的自定义 code、code/domain/retryable 映射不一致被拒绝；标准 JSON-RPC 错误保持有效。
6. `00.1.0`、数字预发布前导零等非规范 SemVer 被拒绝，规范预发布和构建元数据通过。
7. `WAVE-1-INDEPENDENT-PROTOCOL-AUDIT.md` 第 2 节列出的 pnpm、Cargo locked、构建、安全扫描和生成物漂移检查全部通过。

## 非目标

- 不新增 Adapter 方法、状态、权限、错误码或业务能力；
- 不改变技术栈、数据库或生成链路；
- 不实现 supervisor、真实 Adapter 或 Wave 2 节点；
- 不修改 `sources/`。
