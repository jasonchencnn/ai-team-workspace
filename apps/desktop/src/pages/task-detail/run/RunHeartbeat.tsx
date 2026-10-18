// 执行中的心跳指示：证明这次执行还在被监控（D23）。
// 三件事——「监控中」的动态点、已运行时长（每秒走动）、最近一次有新动静多久之前。
// 实时事件通道不可用时（订阅被拒）说明已经改由定时刷新；超过一分钟没有新动静时
// 给一句温和提醒，但不判定执行失败。
import { formatRunClock, formatSecondsAgo, runElapsedSeconds, runStaleHint } from "./runUi.js";
import type { Run } from "../../../bindings/Run.js";

interface RunHeartbeatProps {
  run: Run;
  /** 最近一次收到事件或出现新进度的时间（毫秒时间戳）；还没有过为 null。 */
  lastActivityAt: number | null;
  /** 实时事件通道是否可用。 */
  realtime: boolean;
  /** 当前时间，由页面每秒推进。 */
  now: number;
}

export function RunHeartbeat({ run, lastActivityAt, realtime, now }: RunHeartbeatProps) {
  const elapsed = runElapsedSeconds(run, now);
  const idle =
    lastActivityAt === null
      ? Math.floor(elapsed)
      : Math.max(0, Math.round((now - lastActivityAt) / 1000));
  const stale = runStaleHint(idle);

  return (
    <div className="run-heartbeat" data-testid="run-heartbeat">
      <span className="run-heartbeat__live" data-testid="run-heartbeat-live">
        <span className="run-heartbeat__dot" aria-hidden="true" />
        监控中
      </span>
      <span className="run-heartbeat__clock" data-testid="run-heartbeat-clock">
        已运行 {formatRunClock(elapsed)}
      </span>
      <span className="run-heartbeat__since" data-testid="run-heartbeat-since">
        {lastActivityAt === null ? "最近更新 ——" : `最近更新 ${formatSecondsAgo(idle)}`}
      </span>
      {realtime ? null : (
        <span className="run-heartbeat__fallback" data-testid="run-heartbeat-fallback">
          实时通道不可用，已改为定时刷新
        </span>
      )}
      {stale === null ? null : (
        <span className="run-heartbeat__stale" data-testid="run-heartbeat-stale">
          {stale}
        </span>
      )}
    </div>
  );
}
