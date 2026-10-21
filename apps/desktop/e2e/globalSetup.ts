// 端到端测试的全局准备：临时 Git 仓库 + 测试桥接（真实内核、临时数据库）+ vite（e2e 模式）。
// 返回的函数在全部用例结束后清理进程和临时目录。
import { type ChildProcess, execFileSync, spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { BRIDGE_PORT, UI_PORT } from "./ports.js";

const repoRoot = fileURLToPath(new URL("../../../", import.meta.url));
const desktopDir = fileURLToPath(new URL("../", import.meta.url));

function git(cwd: string, ...args: string[]): void {
  execFileSync(
    "git",
    [
      "-c",
      "user.name=E2E",
      "-c",
      "user.email=e2e@example.invalid",
      "-c",
      "commit.gpgsign=false",
      ...args,
    ],
    { cwd, stdio: "ignore" },
  );
}

function writeRepoFile(root: string, relative: string, content: string): void {
  const target = join(root, relative);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, content);
}

async function waitFor(url: string, init?: RequestInit): Promise<void> {
  const deadline = Date.now() + 120_000;
  for (;;) {
    try {
      await fetch(url, init);
      return;
    } catch {
      if (Date.now() > deadline) {
        throw new Error(`Timed out waiting for ${url}`);
      }
      await new Promise((resolve) => setTimeout(resolve, 300));
    }
  }
}

function stop(child: ChildProcess | undefined): void {
  if (child !== undefined && !child.killed) {
    child.kill("SIGTERM");
  }
}

export default async function globalSetup(): Promise<() => void> {
  const root = mkdtempSync(join(tmpdir(), "teamspace-e2e-"));
  const workspace = join(root, "workspace");
  mkdirSync(workspace);
  git(workspace, "init", "-q", "-b", "main");
  writeRepoFile(workspace, "README.md", "# E2E 项目\n");
  writeRepoFile(workspace, "docs/plan.md", "# 计划\n\n## 注册流程\n");
  writeRepoFile(workspace, "docs/api.md", "# 接口\n");
  // 仓库子目录：也能当工作空间，路径与文件搜索都以它为根。
  writeRepoFile(workspace, "packages/app/package.json", '{ "name": "app" }\n');
  writeRepoFile(workspace, "packages/app/src/main.ts", "export {};\n");
  git(workspace, "add", ".");
  git(workspace, "commit", "-q", "-m", "docs: seed e2e workspace");

  // 桥接在 pick_workspace_folder 时读取该变量，模拟用户在对话框里选了这个目录。
  process.env.E2E_WORKSPACE = workspace;
  process.env.E2E_SUBDIR_WORKSPACE = join(workspace, "packages", "app");

  // 普通文件夹（不在任何 Git 仓库内）：用例里用「手动输入路径 + 检测」切换到它。
  const plain = join(root, "plain-folder");
  writeRepoFile(plain, "notes/outline.md", "# 素材大纲\n");
  writeRepoFile(plain, "素材清单.txt", "镜头 1\n");
  process.env.E2E_PLAIN_WORKSPACE = plain;

  // 备份存放目录：桥接在 pick_backup_dest_dir 时返回它，模拟用户在对话框里选了这个目录。
  const backups = join(root, "backups");
  mkdirSync(backups, { recursive: true });
  process.env.E2E_BACKUP_DIR = backups;

  execFileSync("cargo", ["build", "-q", "--locked", "-p", "teamspace-e2e-bridge"], {
    cwd: repoRoot,
    stdio: "inherit",
  });
  // 「接入 Leader」卡片要写 .mcp.json 时需要 teamspace-mcp 可执行文件：
  // 开发模式的产物在 target/debug/，构建出来写入按钮才可用。
  execFileSync("cargo", ["build", "-q", "--locked", "-p", "teamspace-mcp"], {
    cwd: repoRoot,
    stdio: "inherit",
  });
  const bridge = spawn(
    "cargo",
    [
      "run",
      "-q",
      "--locked",
      "-p",
      "teamspace-e2e-bridge",
      "--",
      join(root, "teamspace.db"),
      String(BRIDGE_PORT),
    ],
    { cwd: repoRoot, stdio: "inherit", env: process.env },
  );

  const vite = spawn(
    "pnpm",
    ["exec", "vite", "--mode", "e2e", "--host", "127.0.0.1", "--port", String(UI_PORT)],
    {
      cwd: desktopDir,
      stdio: "inherit",
      env: { ...process.env, VITE_E2E_BRIDGE: `http://127.0.0.1:${String(BRIDGE_PORT)}` },
    },
  );

  try {
    await waitFor(`http://127.0.0.1:${String(BRIDGE_PORT)}/invoke`, { method: "OPTIONS" });
    await waitFor(`http://127.0.0.1:${String(UI_PORT)}/`);
  } catch (error) {
    stop(bridge);
    stop(vite);
    throw error;
  }

  return () => {
    stop(bridge);
    stop(vite);
    rmSync(root, { recursive: true, force: true });
  };
}
