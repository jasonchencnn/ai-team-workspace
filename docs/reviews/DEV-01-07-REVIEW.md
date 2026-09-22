# DEV-01-07 Rust Schema → TypeScript 类型生成

- 执行模型：`gpt-5.6-sol`
- 当前结论：**PASS**
- 关联基线：DEV-01-07；Gate B 的“Rust Schema → TypeScript 生成无漂移”；ADR-0001 的“Rust DTO + JSON Schema 是单一来源”。
- 范围与非目标：建立 Rust DTO、Schemars Schema、TypeScript 生成物与漂移检查；不反向转换冻结 Adapter Schema，不定义 Actor wire shape，不新增领域状态、权限、错误码或 Adapter 语义。

## 实现

- `crates/contracts` 定义已冻结格式的通用 `ErrorEnvelope`，由 `schemars::JsonSchema` 派生 JSON Schema；错误码仍以 `ERROR_CATALOG.md` 为事实源，本节点不重复枚举。
- `generate-contracts` Rust binary 将 Schema 和由该 Schema 映射出的严格 TypeScript 写入指定目录；不含 `any`，相同输入按稳定字段顺序生成相同字节。
- `packages/contracts-ts/index.ts` 与 `schema/*.json` 明确带生成标记，README 要求前端导入生成类型，禁止手写重复 DTO。
- `contracts:generate` 更新生成物；`contracts:check` 生成到系统临时目录后逐字节比较，仅检查受管的 `index.ts` 与 `schema/`，漂移时退出码非零且自动清理临时目录。
- 生成脚本在 Rust 输出后使用项目本地 Prettier 格式化受管的 `index.ts` 与 `schema/**/*.json`；正式生成和临时漂移检查共享同一后处理，不要求手改或忽略生成物。

## 验证证据

- 临时隔离 Cargo 工程（未写共享 `Cargo.lock`）：`cargo test` 通过；Rust 生成器测试 1/1 通过；成功生成 committed Schema/TypeScript。
- `pnpm run test:unit`：4/4 通过，其中覆盖相同生成树无漂移、人工修改可识别、人工修改时检查进程退出码为 1。
- 正向 CLI：`node scripts/check-contracts.mjs packages/contracts-ts packages/contracts-ts`，退出码 0，输出 `Generated contracts are up to date.`。
- 负向 CLI：复制生成物到临时目录并篡改 `index.ts` 后执行同一 checker，退出码 1，输出 `Generated contracts are stale: index.ts`；临时目录已清理。
- `pnpm run typecheck`、contracts-ts 包定向 typecheck、相关 ESLint 均通过。
- 总控统一生成 `Cargo.lock`/`pnpm-lock.yaml` 后，`pnpm install --frozen-lockfile`、全局 fmt/typecheck/lint、unit 4/4、前端 build 均通过。
- 使用现有锁文件复核：`cargo test -p contracts --locked`（生成器 1/1）、contracts 定向 Clippy `-D warnings`、`pnpm contracts:check`、contracts-ts 定向 typecheck 均通过。
- 最终统一格式检查发现 Schema 原始 pretty-print 与项目 Prettier 风格不同后，已将 Prettier 纳入生成链路并重新生成；`pnpm contracts:check`、unit、typecheck 及生成物定向 `prettier --check` 再次通过。

## 安全、兼容与回退

- 生成器仅写调用方给定的输出目录；正式 checker 使用 `mkdtemp`，不修改已提交生成物。
- `details` 保持 `Record<string, unknown>`，不泄露或臆造错误详情结构；允许键仍由错误目录白名单约束。
- 未修改冻结 Schema、`sources/`、domain、desktop、根 Cargo workspace 或任何 lockfile。
- 回退可移除 contracts DTO/生成 binary、contracts-ts 包和两个根脚本入口；没有数据迁移或运行时状态。

## 最终审核结论

统一锁文件与 frozen/locked 门禁已经复核：

- Rust DTO + Schemars 是生成契约的唯一源码；
- TypeScript 与 JSON Schema 生成可重现；
- committed 生成物漂移会稳定返回非零；
- 前端包不包含重复手写 DTO；
- locked/frozen 专属检查与通用质量门禁均通过。

结论：**PASS — DEV-01-07 节点完成，可供 DEV-03-01 与 DEV-05-11 依赖。**
