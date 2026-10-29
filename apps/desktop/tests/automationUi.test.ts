import { describe, expect, it } from "vitest";

import { lines, toDraft, toInput } from "../src/pages/settings/automationUi.js";

describe("团队自动化设置表单", () => {
  it("列表按每行一项编辑，空行与首尾空白去掉", () => {
    expect(lines("  docs/DECISIONS.md \n\n.github/\n")).toEqual(["docs/DECISIONS.md", ".github/"]);
  });

  it("设置与表单来回转换不丢内容，空检查命令存为 null", () => {
    const draft = toDraft({
      teamId: "t",
      autonomyLevel: 2,
      checkCommand: "scripts/check.sh",
      protectedPaths: ["AGENTS.md", "Cargo.toml"],
      allowCommands: ["make test"],
      denyCommands: [],
      notifyLeader: true,
      updatedAt: null,
    });
    expect(toInput("t", draft)).toEqual({
      teamId: "t",
      autonomyLevel: 2,
      checkCommand: "scripts/check.sh",
      protectedPaths: ["AGENTS.md", "Cargo.toml"],
      allowCommands: ["make test"],
      denyCommands: [],
      notifyLeader: true,
    });
    expect(toInput("t", { ...draft, checkCommand: "   " }).checkCommand).toBeNull();
  });
});
