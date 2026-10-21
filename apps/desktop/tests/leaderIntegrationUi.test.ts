// 「接入 Leader」卡片的纯逻辑：三段配置、复制按钮状态、权限边界文案与写入提示。
// 卡片组件本身按这些结果渲染，界面上的行为由端到端用例覆盖。
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import type { LeaderIntegration } from "../src/bindings/LeaderIntegration.js";
import {
  LEADER_CAN_DO,
  LEADER_CANNOT_DO,
  canWriteMcpConfig,
  configSections,
  copyLabel,
  gitNotice,
  writeBlockedHint,
  writeConfirmText,
  writeDoneText,
} from "../src/pages/team-home/leaderIntegrationUi.js";

const cardPath = fileURLToPath(
  new URL("../src/pages/team-home/LeaderIntegrationCard.tsx", import.meta.url),
);

function integration(overrides: Partial<LeaderIntegration> = {}): LeaderIntegration {
  return {
    teamId: "team-1",
    binaryPath: "/repo/target/release/teamspace-mcp",
    binaryFound: true,
    binaryHint: null,
    controlSocket: "/data/teamspace-control.sock",
    workspacePath: "/repo",
    gitWorkspace: false,
    claudeCommand: "claude mcp add teamspace -- /repo/target/release/teamspace-mcp",
    mcpJson: '{\n  "mcpServers": {\n    "teamspace": {}\n  }\n}',
    codexToml: '[mcp_servers.teamspace]\ncommand = "/repo/target/release/teamspace-mcp"\n',
    ...overrides,
  };
}

describe("configSections", () => {
  const sections = configSections(integration());

  it("给出三段配置，文本取自后端", () => {
    expect(sections.map((section) => section.key)).toEqual(["claude", "mcpJson", "codex"]);
    expect(sections[0]?.text).toBe(
      "claude mcp add teamspace -- /repo/target/release/teamspace-mcp",
    );
    expect(sections[1]?.text).toContain("mcpServers");
    expect(sections[2]?.text).toContain("[mcp_servers.teamspace]");
  });

  it("每段都带标题与说明", () => {
    for (const section of sections) {
      expect(section.title.length).toBeGreaterThan(0);
      expect(section.hint.length).toBeGreaterThan(0);
    }
    expect(sections[1]?.title).toContain(".mcp.json");
    expect(sections[2]?.title).toContain("config.toml");
  });
});

describe("copyLabel", () => {
  it("按复制状态给出按钮文字", () => {
    expect(copyLabel("idle")).toBe("复制");
    expect(copyLabel("ok")).toBe("已复制");
    expect(copyLabel("failed")).toBe("复制失败");
  });
});

describe("权限边界文案", () => {
  it("说明 Leader 能做的事", () => {
    expect(LEADER_CAN_DO.length).toBeGreaterThan(0);
    expect(LEADER_CAN_DO.join(" ")).toContain("创建");
    expect(LEADER_CAN_DO.join(" ")).toContain("验收");
  });

  it("说明合并与权限答复只能由人来做（D26）", () => {
    const text = LEADER_CANNOT_DO.join(" ");
    expect(text).toContain("合并");
    expect(text).toContain("权限");
    expect(text).toContain("Harness");
  });
});

describe("提示文案", () => {
  it("Git 工作空间提示文件会出现在改动里", () => {
    expect(gitNotice(true)).toContain("Git 改动");
    expect(gitNotice(true)).toContain(".gitignore");
    expect(gitNotice(false)).toBeNull();
  });

  it("写入前说明写到哪里、会不会覆盖别的内容", () => {
    const text = writeConfirmText(integration({ workspacePath: "/repo" }));
    expect(text).toContain("/repo/.mcp.json");
    expect(text).toContain("保留");
    // 键顺序可能被重排，要写清楚。
    expect(text).toContain("键顺序");
    // 写入前会备份原文件。
    expect(text).toContain(".mcp.json.bak");
  });

  it("写入完成后区分新建与合并", () => {
    expect(writeDoneText("/repo/.mcp.json", true)).toContain("已新建");
    expect(writeDoneText("/repo/.mcp.json", false)).toContain("合并");
  });

  it("没构建出 teamspace-mcp 时不能写入", () => {
    const missing = integration({ binaryFound: false, binaryHint: "请先构建 teamspace-mcp" });
    expect(canWriteMcpConfig(missing)).toBe(false);
    expect(writeBlockedHint(missing)).toContain("不能写入");
    // 找到可执行文件时照常写。
    expect(canWriteMcpConfig(integration())).toBe(true);
    expect(writeBlockedHint(integration())).toBeNull();
  });
});

describe("卡片组件", () => {
  it("渲染三段配置、复制按钮与写入按钮", async () => {
    const source = await readFile(cardPath, "utf8");
    expect(source).toContain("configSections(data)");
    expect(source).toContain("copyLabel(state)");
    expect(source).toContain("navigator.clipboard.writeText");
    expect(source).toContain("写入工作空间 .mcp.json");
    expect(source).toContain("确认写入");
  });

  it("没找到可执行文件时禁用写入按钮", async () => {
    const source = await readFile(cardPath, "utf8");
    expect(source).toContain("disabled={!writable}");
    expect(source).toContain("canWriteMcpConfig(data)");
  });
});
