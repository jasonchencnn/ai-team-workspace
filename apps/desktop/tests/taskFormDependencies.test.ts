import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { checkTaskDependencies } from "../src/api/commands.js";
import { TaskFormPage } from "../src/pages/task-form/TaskFormPage.js";

const state = vi.hoisted(() => ({
  hookIndex: 0,
  selection: [] as string[],
  debounced: null as string[] | null,
  fetching: false,
  cycle: [] as string[],
  query: null as null | { queryKey: unknown[]; queryFn: () => Promise<unknown> },
}));

vi.mock("react", async (importOriginal) => {
  const react = await importOriginal<typeof import("react")>();
  return {
    ...react,
    useState: (initial: unknown) => {
      const index = state.hookIndex++;
      if (index === 5) {
        return react.useState(state.selection);
      }
      if (index === 9) {
        return react.useState(state.debounced ?? [...state.selection].sort());
      }
      return react.useState(initial);
    },
  };
});

vi.mock("react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("react-router")>()),
  useParams: () => ({ teamId: "team" }),
  useNavigate: () => vi.fn(),
}));

vi.mock("@tanstack/react-query", () => ({
  useQuery: (options: NonNullable<typeof state.query>) => {
    if (options.queryKey[0] === "task-dependency-check") {
      state.query = options;
      return {
        data: { cycle: state.cycle, readinessNotice: "后端就绪说明" },
        isSuccess: true,
        isFetching: state.fetching,
      };
    }
    const data = options.queryKey.includes("detail") ? undefined : [];
    return { data, isPending: false };
  },
  useMutation: () => ({ isPending: false, isError: false }),
  useQueryClient: () => ({}),
}));

vi.mock("../src/api/commands.js", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../src/api/commands.js")>()),
  checkTaskDependencies: vi.fn().mockResolvedValue({ cycle: [], readinessNotice: "后端就绪说明" }),
}));

function render() {
  state.hookIndex = 0;
  return renderToStaticMarkup(createElement(TaskFormPage));
}

describe("任务表单依赖预检", () => {
  beforeEach(() => {
    state.selection = [];
    state.debounced = null;
    state.fetching = false;
    state.cycle = [];
    state.query = null;
    vi.clearAllMocks();
  });

  it("排序后生成稳定的查询键和预检参数，不修改选择数组", async () => {
    state.selection = ["second", "first"];
    render();
    const originalKey = state.query?.queryKey;
    expect(originalKey).toEqual(["task-dependency-check", "team", null, ["first", "second"]]);
    await state.query?.queryFn();
    expect(checkTaskDependencies).toHaveBeenCalledWith({
      teamId: "team",
      taskId: null,
      dependsOn: ["first", "second"],
    });
    expect(state.selection).toEqual(["second", "first"]);
    state.selection = ["first", "second"];
    render();
    expect(state.query?.queryKey).toEqual(originalKey);
  });

  it("防抖期间禁用保存，隐藏旧环路结果，查询仍使用防抖前的选择", () => {
    state.selection = ["new"];
    state.debounced = ["old"];
    state.cycle = ["FE-001", "FE-001"];
    const html = render();
    expect(html).toMatch(/<button[^>]*type="submit"[^>]*disabled=""/);
    expect(html).toContain("正在检查依赖");
    expect(html).not.toContain("不能保存：依赖形成了循环");
    expect(state.query?.queryKey[3]).toEqual(["old"]);
  });

  it("防抖结束后，网络预检未完成仍禁用保存", () => {
    state.selection = ["first"];
    state.fetching = true;
    const html = render();
    expect(html).toMatch(/<button[^>]*type="submit"[^>]*disabled=""/);
    expect(html).toContain("正在检查依赖");
  });

  it("仅改变选择顺序不视为防抖中，无环结果允许保存", () => {
    state.selection = ["second", "first"];
    state.debounced = ["first", "second"];
    const html = render();
    expect(html).not.toMatch(/<button[^>]*type="submit"[^>]*disabled=""/);
    expect(html).not.toContain("正在检查依赖");
    expect(html).toContain("后端就绪说明");
  });
});
