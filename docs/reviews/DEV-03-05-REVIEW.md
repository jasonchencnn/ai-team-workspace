# DEV-03-05 Review: Mock Fault Injection

## 1. 目标与实现范围
依据 `docs/specs/adapter/v0/PROTOCOL.md`（§2、§6、§7）、`docs/specs/ERROR_CATALOG.md` 以及 `docs/development/DEVELOPMENT_NODES.md`，完成 Mock Adapter 故障注入机制与 Supervisor 鲁棒性验证。

- **故障注入场景覆盖**：
  1. `timeout`（握手超时与运行超时）：模拟进程无响应，Supervisor 在限定时间触发 `TimedOut`（`-32017 ADAPTER_UNAVAILABLE`）；
  2. `duplicate`：连续发送相同 sequence 相同 payload 的通知，验证 Supervisor 幂等忽略；构造相同 sequence 冲突 payload，验证抛出 `-32099 ADAPTER_PROTOCOL_VIOLATION`；
  3. `out_of_order`：故意乱序发送 sequence（如 sequence 2 先于 1），验证准确诊断为 `-32015 RUN_OUTPUT_OUT_OF_ORDER`；
  4. `crash`：运行中途非零退出，Supervisor 准确识别 `ProcessExited`，`-32017 ADAPTER_UNAVAILABLE` 并捕获 stderr 脱敏诊断；
  5. `late_callback`：在终态 `run.completed` 后发送晚到通知，验证严格被拒并诊断为 `-32099 ADAPTER_PROTOCOL_VIOLATION`；
  6. `reject_cancel` / `reject_cancel_timeout`：模拟 Adapter 显式拒绝取消或取消超时，验证 Supervisor 安全处理并清理子进程；
  7. `unresponsive`：模拟管道异常关闭与失联，准确归类错误；
  8. `run.cancel` 正常基线与模型结构定义。

- **看门狗保障（Deadline / Watchdog）**：
  - 每个故障注入测试均包裹在 5 秒 `tokio::time::timeout` 内，严防测试挂死 CI；
  - 验证 Supervisor 在超时或异常退出后确定性回收清理子进程，无孤儿进程残留。

## 2. 验证结果
- **Command Output**:
  - `cargo test -p adapter-supervisor --locked`: 59 passed, 0 failed, 0 ignored.
  - `cargo clippy -p adapter-supervisor --all-targets --all-features --locked -- -D warnings`: 0 warnings, 0 errors.
  - `cargo fmt -p adapter-supervisor -- --check`: clean.

## 3. 验收结论
DEV-03-05 自验收通过 (`SELF_CHECK_PASS`)。
