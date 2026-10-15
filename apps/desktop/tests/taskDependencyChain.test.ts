import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";

import type { TaskDetail } from "../src/bindings/TaskDetail.js";
import { TaskDependencyChain } from "../src/components/TaskDependencyChain.js";
import { TaskDependencies } from "../src/pages/task-detail/TaskDependencies.js";

const first = { id: "first", key: "BE-018", title: "Auth API", status: "done" as const };
const second = { id: "second", key: "FE-023", title: "登录界面", status: "review" as const };
const current = { id: "current", key: "FE-024", title: "前后端联调", status: "todo" as const };
const downstream = { id: "downstream", key: "QA-006", title: "联调验收", status: "todo" as const };

function render(preview = false, empty = false) {
  return renderToStaticMarkup(
    createElement(
      MemoryRouter,
      null,
      createElement(TaskDependencyChain, {
        teamId: "team",
        current,
        dependencies: empty ? [] : [first, second],
        dependents: empty ? [] : [downstream],
        statusLabel: (status) => `后端标签:${status}`,
        preview,
      }),
    ),
  );
}

describe("TaskDependencyChain", () => {
  it("按前置、当前、直接下游显示真实任务和后端状态标签", () => {
    const html = render();
    expect(html).toContain('aria-label="依赖链"');
    expect(html).toContain("前置 2 个 · 下游 1 个");
    expect(html.indexOf(first.key)).toBeLessThan(html.indexOf(current.key));
    expect(html.indexOf(current.key)).toBeLessThan(html.indexOf(downstream.key));
    for (const task of [first, second, current, downstream]) {
      expect(html).toContain(task.key);
      expect(html).toContain(task.title);
      expect(html).toContain(`后端标签:${task.status}`);
    }
  });

  it("详情中上下游可跳转，当前任务不重复提供链接", () => {
    const html = render();
    expect(html).toContain('href="/teams/team/tasks/first"');
    expect(html).toContain('href="/teams/team/tasks/downstream"');
    expect(html).not.toContain('href="/teams/team/tasks/current"');
    expect(html).toContain("当前任务");
  });

  it("表单预览逐条显示依赖方向，且不会导航离开未保存表单", () => {
    const html = render(true);
    expect(html).toContain('aria-label="依赖链预览"');
    expect(html.match(/class="dependency-chain__row"/g)).toHaveLength(3);
    expect(html.match(/aria-label="指向"/g)).toHaveLength(3);
    expect(html).not.toContain("href=");
    expect(html).toContain("下游");
  });

  it("无依赖时展示明确空态", () => {
    const html = render(false, true);
    expect(html).toContain("无前置任务");
    expect(html).toContain("无下游任务");
    expect(render(true, true)).toContain("暂无依赖关系");
  });
});

describe("TaskDependencies", () => {
  const detail: TaskDetail = {
    task: {
      ...current,
      teamId: "team",
      description: "",
      ownerRoleId: "role",
      fileRefs: [],
      dependsOn: [],
      blockedReason: null,
      createdAt: "2026-10-15T00:00:00Z",
      updatedAt: "2026-10-15T00:00:00Z",
    },
    owner: { id: "role", name: "前端", code: "frontend", taskPrefix: "FE", archived: false },
    fileRefs: [],
    dependencies: [],
    dependents: [],
    allowedTransitions: [],
    readiness: [],
    events: [],
    latestRun: null,
    runProblems: [],
    dependencyNotice: null,
  };

  it("自动就绪文案直接取后端，不根据前端状态推断", () => {
    const renderNotice = (notice: string | null) =>
      renderToStaticMarkup(
        createElement(
          MemoryRouter,
          null,
          createElement(TaskDependencies, {
            detail: { ...detail, dependencyNotice: notice },
            statusLabel: (status) => status,
          }),
        ),
      );
    expect(renderNotice(null)).not.toContain("自动就绪");
    const html = renderNotice("后端说明：引用文件可选，填写后须存在。");
    expect(html).toContain("自动就绪");
    expect(html).toContain("后端说明：引用文件可选，填写后须存在。");
  });
});
