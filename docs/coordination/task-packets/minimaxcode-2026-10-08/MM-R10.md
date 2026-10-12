# MM-R10｜补齐有意义的领域边界及序列化测试

- 关联节点：DEV-02-11，关联02-12/13/14及03-06。主责Minimaxcode，实际模型/版本启动回填，不猜测M3；Codex独立复核。
- 难度：R2测试设计，依据涉及R3，最终语义由Codex复核。
- 状态：既有已交付纯规则的测试补充可执行；不解除节点/新实现依赖阻断。
- 授权：Human本轮接续指令、ADR-0008；HEAD与共享工作树按总交接表，启动重新固定输入SHA。
- 必读：AGENTS/override、四开发基线、specs/README、本任务对应STATE_MACHINES/PERMISSION_MATRIX/DATA_CONVENTIONS/ERROR_CATALOG、ADVERSARIAL_REVIEW_PLAYBOOK及本轮深入审计。

## 写集

仅允许新增 `crates/domain/tests/deep_audit_coverage.rs`、`docs/coordination/minimax-return/MM-R10-RESULT.md` 和 `docs/reviews/evidence/minimax-return/MM-R10/`。未列文件只读；不修改production逻辑、旧migration、manifest/lockfile、历史evidence、正式状态或ADR接受状态；不提交/推送/部署/联网/下载，不访问生产凭据或数据。新增费用$0。

## 实测起点与目标

同字节domain16份src在现有测试加两个边界测试下，1907/2197行=86.80%，未达90%。application只有占位，不能计100%；region84.75%不是branch，分支85%仍未验证。本机Apple21工具已能读取RustLLVM22的实际profile与covmap，不需要为了行覆盖率再下载工具。

低覆盖范围：

- `crates/domain/src/run_state_machine.rs`：73.68%
- `crates/domain/src/permission_matrix.rs`：74.62%
- `crates/domain/src/task_state_machine.rs`：75.95%
- `crates/domain/src/run_termination.rs`：82.61%
- `crates/domain/src/permission_intersection.rs`：83.62%
- `crates/domain/src/idempotency.rs`：85.29%
- `crates/domain/src/attempt_state_machine.rs`：87.18%
- `crates/domain/src/approval_state_machine.rs`：87.69%
- `crates/domain/src/binding_state_machine.rs`：88.24%
- `crates/domain/src/message_state_machine.rs`：89.40%

先读 `docs/reviews/evidence/deep-audit-2026-10-08/coverage-tools/domain-export.json` 定位未覆盖语句，再按冻结需求设计拒绝条件、serde/wire码、边界和合法对照。不要抄实现做期望、只刷getter/Display、伪造context或加无意义测试。覆盖缺口不等于已确认产品bug；若发现真实bug，另存失败回归并停止扩大写集，回传Codex审阅。

## 验收

```text
cargo test -p teamspace-domain --test deep_audit_coverage --locked --offline
```

复测借用本轮 `coverage-tools/run_domain_coverage.py` 的离线方法，**在新的证据目录重新复制当前workspace源码与完整测试（包括本包新增测试），重新固定manifest/lock/source SHA，调整runner的repro、target、profile及输出路径**；不覆盖旧profile/log/summary。使用当前全部测试和固定的生产src分母，包含未执行函数映射，不排除低覆盖生产文件；记录编译工具/version、RUSTFLAGS、对象清单、profile、源码hash、每文件/汇总。代码变化需重新测量，不沿用旧数。

只有实测domain行达到90%才可声称该项通过；分支若无真实映射必须标未验证，不能写0分支所以100%、不能用region替代。全体Rust/TS80%、application真实代码覆盖及节点完整DONE仍需要后续验收。

回传SELF_CHECK_PASS / SELF_CHECK_BLOCKED、实际模型、输入/输出SHA、修改文件、冻结依据、每个测试为何能区分错误实现、命令/exit、完整日志、覆盖率明细、未运行/失败/风险。自验收不等于DONE；规格冲突、新状态/权限/码/协议、外部权限/费用、写集冲突时停止并上报。
