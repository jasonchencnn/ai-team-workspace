// 任务面板的视图与筛选存在地址栏里：这里验证地址栏与面板状态之间的换算，
// 包括从别处（活动页）进详情时只把面板自己的参数带回面板。
import { describe, expect, it } from "vitest";

import {
  DEFAULT_PANEL,
  parseTaskPanel,
  serializeTaskPanel,
  taskPanelSearch,
} from "../src/pages/tasks/taskUi.js";

const parse = (search: string) => parseTaskPanel(new URLSearchParams(search));

describe("parseTaskPanel", () => {
  it("没有参数时是默认的看板视图与全部筛选", () => {
    expect(parse("")).toEqual(DEFAULT_PANEL);
  });

  it("读出视图、首列筛选和两个勾选", () => {
    expect(parse("view=list&filter=ready&done=1&cancelled=1")).toEqual({
      view: "list",
      filter: "ready",
      showDone: true,
      showCancelled: true,
    });
  });

  it("无法识别的取值按默认值处理，不报错", () => {
    expect(parse("view=gantt&filter=done&done=yes&cancelled=true")).toEqual(DEFAULT_PANEL);
  });
});

describe("serializeTaskPanel", () => {
  it("默认值不写进地址栏，链接保持干净", () => {
    expect(serializeTaskPanel(DEFAULT_PANEL).toString()).toBe("");
  });

  it("非默认值按固定顺序写出", () => {
    expect(
      serializeTaskPanel({
        view: "list",
        filter: "ready",
        showDone: true,
        showCancelled: false,
      }).toString(),
    ).toBe("view=list&filter=ready&done=1");
  });

  it("往返换算后状态不变", () => {
    const state = { view: "list", filter: "todo", showDone: true, showCancelled: true } as const;
    expect(parse(serializeTaskPanel(state).toString())).toEqual(state);
  });
});

describe("taskPanelSearch", () => {
  it("带上面板参数时返回可直接拼接的搜索串", () => {
    expect(taskPanelSearch("?view=list&filter=ready")).toBe("?view=list&filter=ready");
  });

  it("全是默认值时返回空串，面板地址不带问号", () => {
    expect(taskPanelSearch("")).toBe("");
    expect(taskPanelSearch("?view=board&filter=all")).toBe("");
  });

  it("别的页面的参数不跟着带回面板", () => {
    expect(taskPanelSearch("?task=abc&kind=task_created&view=list")).toBe("?view=list");
  });
});
