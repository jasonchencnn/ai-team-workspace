// 共享文件页的纯逻辑：目录树分组与搜索、文件内链的放行范围。
// 页面只渲染这些结果；边界判断放在这里，才能在不开界面的情况下验证「什么能点」。
import { describe, expect, it } from "vitest";

import {
  ancestorsOf,
  buildTree,
  filterTree,
  formatSize,
  formatUpdated,
  isMarkdownRef,
  openDirectories,
  resolveDocLink,
} from "../src/pages/files/filesUi.js";

describe("buildTree", () => {
  it("keeps root files first and counts files per directory", () => {
    const tree = buildTree([
      "plans/task.md",
      "README.md",
      "plans/brief.md",
      "docs/deep/note.md",
      "docs/spec.md",
    ]);

    expect(tree.map((node) => [node.name, node.isDirectory])).toEqual([
      ["README.md", false],
      ["docs", true],
      ["plans", true],
    ]);
    const docs = tree[1];
    expect(docs?.count).toBe(2);
    // 与根层同一套排序：文件在前，目录在后。
    expect(docs?.children.map((node) => node.name)).toEqual(["spec.md", "deep"]);
    expect(docs?.children[1]?.count).toBe(1);
  });

  it("returns nothing for an empty workspace", () => {
    expect(buildTree([])).toEqual([]);
  });
});

describe("filterTree", () => {
  const tree = buildTree(["README.md", "plans/task.md", "plans/roadmap.md", "docs/spec.md"]);

  it("drops files that do not match and directories left empty", () => {
    const filtered = filterTree(tree, "roadmap");
    expect(filtered.map((node) => node.name)).toEqual(["plans"]);
    expect(filtered[0]?.children.map((node) => node.name)).toEqual(["roadmap.md"]);
  });

  it("keeps the whole subtree when the directory name matches", () => {
    const filtered = filterTree(tree, "plans");
    expect(filtered).toHaveLength(1);
    expect(filtered[0]?.children).toHaveLength(2);
  });

  it("returns everything for a blank query", () => {
    expect(filterTree(tree, "  ")).toEqual(tree);
  });
});

describe("directory expansion", () => {
  it("lists every directory of a filtered tree so search results are visible", () => {
    const filtered = filterTree(buildTree(["plans/task.md", "docs/spec.md"]), "plan");
    expect(openDirectories(filtered)).toEqual(["plans"]);
  });

  it("gives the ancestors of the selected file, root first", () => {
    expect(ancestorsOf("docs/deep/note.md")).toEqual(["docs", "docs/deep"]);
    expect(ancestorsOf("README.md")).toEqual([]);
  });
});

describe("resolveDocLink", () => {
  const current = "plans/task-breakdown.md";

  it("opens markdown links relative to the current file", () => {
    expect(resolveDocLink(current, "brief.md")).toEqual({
      kind: "doc",
      path: "plans/brief.md",
    });
    expect(resolveDocLink(current, "../docs/spec.md#接口")).toEqual({
      kind: "doc",
      path: "docs/spec.md",
    });
    expect(resolveDocLink("docs/deep/note.md", "./other.md")).toEqual({
      kind: "doc",
      path: "docs/deep/other.md",
    });
  });

  it("keeps in-page anchors on the current file", () => {
    expect(resolveDocLink(current, "#验收")).toEqual({ kind: "doc", path: current });
  });

  it("blocks external links: the app has no way to open them", () => {
    for (const href of [
      "https://example.com/a.md",
      "http://example.com",
      "mailto:someone@example.com",
    ]) {
      expect(resolveDocLink(current, href)).toEqual({
        kind: "blocked",
        reason: "外部链接，应用内不打开",
      });
    }
  });

  it("blocks absolute paths, escapes above the workspace and non-markdown targets", () => {
    expect(resolveDocLink(current, "/etc/passwd").kind).toBe("blocked");
    expect(resolveDocLink(current, "C:\\notes.md").kind).toBe("blocked");
    expect(resolveDocLink(current, "../../outside.md").kind).toBe("blocked");
    expect(resolveDocLink(current, "diagram.png").kind).toBe("blocked");
    expect(resolveDocLink(current, "specs/api.md").kind).toBe("doc");
    expect(resolveDocLink(current, undefined).kind).toBe("blocked");
  });
});

describe("formatting", () => {
  it("formats sizes the way the page shows them", () => {
    expect(formatSize(0)).toBe("0 B");
    expect(formatSize(999)).toBe("999 B");
    expect(formatSize(3277)).toBe("3.2 KB");
    expect(formatSize(2 * 1024 * 1024)).toBe("2.0 MB");
  });

  it("labels today and yesterday in local time", () => {
    const now = new Date(2026, 9, 15, 18, 30);
    const iso = (date: Date) => date.toISOString();
    expect(formatUpdated(iso(new Date(2026, 9, 15, 14, 20)), now)).toBe("今天 14:20");
    expect(formatUpdated(iso(new Date(2026, 9, 14, 9, 3)), now)).toBe("昨天 09:03");
    expect(formatUpdated(iso(new Date(2026, 9, 1, 8, 0)), now)).toBe("10/01 08:00");
    expect(formatUpdated("不是时间", now)).toBe("不是时间");
  });

  it("recognizes markdown references case-insensitively", () => {
    expect(isMarkdownRef("plans/task.MD")).toBe(true);
    expect(isMarkdownRef("素材清单.txt")).toBe(false);
  });
});
