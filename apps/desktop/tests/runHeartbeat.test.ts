// 心跳指示与轮询兜底的判定（D23）：时间格式、停滞提示、轮询的开合。
// 界面上的文字都来自这些纯函数，单元测试锁住文案与阈值，组件只负责排版。
import { describe, expect, it, vi } from "vitest";

import type { Run } from "../src/bindings/Run.js";
import { queryKeys } from "../src/api/queryKeys.js";
import {
  formatRunClock,
  formatSecondsAgo,
  RUN_STALE_SECONDS,
  runElapsedSeconds,
  runStaleHint,
} from "../src/pages/task-detail/run/runUi.js";
import {
  createRunPoller,
  RUN_POLL_INTERVAL_MS,
  shouldPollRun,
} from "../src/pages/task-detail/run/useRunPolling.js";
import { isRunActive } from "../src/pages/task-detail/run/useRunEvents.js";

/** 只用到状态字段的最小执行记录。 */
function runWith(status: Run["status"], startedAt = "2026-10-18T08:00:00Z"): Run {
  return {
    id: "run-1",
    teamId: "team-1",
    taskId: "task-1",
    taskKey: "FE-001",
    roleId: "role-1",
    harness: { name: "mock", kind: "mock" },
    attempt: 1,
    status,
    statusLabel: status,
    sessionId: null,
    execDir: "/tmp/exec",
    isolation: "plain",
    branch: null,
    baseCommit: null,
    capabilities: null,
    startedAt,
    endedAt: null,
    stopReason: null,
    failure: null,
    failureKind: null,
    failureDetail: null,
    summary: null,
    pendingPermission: null,
    takenOver: false,
    canResume: false,
    isolationNote: null,
    review: null,
    journal: null,
  } as unknown as Run;
}

describe("心跳计时", () => {
  it("从 startedAt 起算，进行中用当前时间", () => {
    const run = runWith("running");
    const started = Date.parse(run.startedAt);
    expect(runElapsedSeconds(run, started + 12_400)).toBe(12);
  });

  it("结束后用 endedAt - startedAt", () => {
    const run = { ...runWith("succeeded"), endedAt: "2026-10-18T08:02:05Z" } as Run;
    expect(runElapsedSeconds(run, Date.parse("2026-10-18T09:00:00Z"))).toBe(125);
  });

  it("时钟按 m:ss 走，超过一小时才带小时位", () => {
    expect(formatRunClock(0)).toBe("0:00");
    expect(formatRunClock(7)).toBe("0:07");
    expect(formatRunClock(65)).toBe("1:05");
    expect(formatRunClock(3599)).toBe("59:59");
    expect(formatRunClock(3600)).toBe("1:00:00");
    expect(formatRunClock(3725)).toBe("1:02:05");
  });

  it("时钟对负数与小数取整，不出现负的已运行", () => {
    expect(formatRunClock(-5)).toBe("0:00");
    expect(formatRunClock(9.9)).toBe("0:09");
  });
});

describe("「最近更新 N 秒前」", () => {
  it("按秒、分、小时分档，不足一秒是「刚刚」", () => {
    expect(formatSecondsAgo(0)).toBe("刚刚");
    expect(formatSecondsAgo(0.4)).toBe("刚刚");
    expect(formatSecondsAgo(12)).toBe("12 秒前");
    expect(formatSecondsAgo(59)).toBe("59 秒前");
    expect(formatSecondsAgo(60)).toBe("1 分钟前");
    expect(formatSecondsAgo(600)).toBe("10 分钟前");
    expect(formatSecondsAgo(7200)).toBe("2 小时前");
  });

  it("负数按 0 处理", () => {
    expect(formatSecondsAgo(-3)).toBe("刚刚");
  });
});

describe("超过 60 秒没有新进度时的提示", () => {
  it("阈值内不提示", () => {
    expect(RUN_STALE_SECONDS).toBe(60);
    expect(runStaleHint(0)).toBeNull();
    expect(runStaleHint(59)).toBeNull();
  });

  it("超过阈值给温和提醒，并说明可以取消或人工接管", () => {
    expect(runStaleHint(60)).toBe("已 60 秒没有新的进度，Agent 可能在长时间思考；可取消或人工接管");
    expect(runStaleHint(125)).toBe(
      "已 2 分 5 秒没有新的进度，Agent 可能在长时间思考；可取消或人工接管",
    );
  });

  it("只提醒不判定失败", () => {
    const hint = runStaleHint(3600);
    expect(hint).toContain("可取消或人工接管");
    expect(hint).not.toContain("失败");
  });
});

describe("失效范围", () => {
  it("任务详情、执行列表与待验收入口在同一个前缀下，一起刷新", () => {
    const task = queryKeys.task("team-1", "task-1");
    for (const key of [queryKeys.runs("team-1", "task-1"), queryKeys.review("team-1", "task-1")]) {
      expect(key.slice(0, task.length)).toEqual(task);
    }
  });
});

describe("轮询兜底的开合", () => {
  it("只有活动执行才轮询", () => {
    expect(shouldPollRun(runWith("queued"))).toBe(true);
    expect(shouldPollRun(runWith("running"))).toBe(true);
    expect(shouldPollRun(runWith("waiting_permission"))).toBe(true);
    expect(shouldPollRun(runWith("succeeded"))).toBe(false);
    expect(shouldPollRun(runWith("failed"))).toBe(false);
    expect(shouldPollRun(runWith("cancelled"))).toBe(false);
    expect(shouldPollRun(runWith("interrupted"))).toBe(false);
    expect(shouldPollRun(null)).toBe(false);
    expect(shouldPollRun(runWith("running"))).toBe(isRunActive(runWith("running")));
  });

  it("间隔是 3 秒", () => {
    expect(RUN_POLL_INTERVAL_MS).toBe(3000);
  });

  it("开始后按间隔触发，停止后不再触发", () => {
    vi.useFakeTimers();
    try {
      const tick = vi.fn();
      const poller = createRunPoller(tick);
      poller.start();
      poller.start(); // 重复开始不会叠加定时器
      vi.advanceTimersByTime(RUN_POLL_INTERVAL_MS);
      expect(tick).toHaveBeenCalledTimes(1);
      vi.advanceTimersByTime(RUN_POLL_INTERVAL_MS * 2);
      expect(tick).toHaveBeenCalledTimes(3);

      poller.stop();
      vi.advanceTimersByTime(RUN_POLL_INTERVAL_MS * 5);
      expect(tick).toHaveBeenCalledTimes(3);
      poller.stop(); // 重复停止也不报错
    } finally {
      vi.useRealTimers();
    }
  });

  it("停止后可以再次开始（执行续接、重新开始执行时）", () => {
    vi.useFakeTimers();
    try {
      const tick = vi.fn();
      const poller = createRunPoller(tick, 1000);
      poller.start();
      vi.advanceTimersByTime(1000);
      poller.stop();
      vi.advanceTimersByTime(5000);
      expect(tick).toHaveBeenCalledTimes(1);

      poller.start();
      vi.advanceTimersByTime(1000);
      expect(tick).toHaveBeenCalledTimes(2);
      poller.stop();
    } finally {
      vi.useRealTimers();
    }
  });
});
