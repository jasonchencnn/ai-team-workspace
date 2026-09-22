import { spawnSync } from "node:child_process";
import { mkdirSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const output = resolve(process.argv[2] ?? resolve(root, "packages/contracts-ts"));
mkdirSync(output, { recursive: true });

const result = spawnSync(
  "cargo",
  ["run", "--quiet", "-p", "contracts", "--bin", "generate-contracts", "--", output],
  { cwd: root, encoding: "utf8", stdio: "inherit" },
);
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);

const managedFiles = [resolve(output, "index.ts"), ...jsonFiles(resolve(output, "schema"))];
const prettier = spawnSync(
  "pnpm",
  ["exec", "prettier", "--write", "--log-level", "silent", ...managedFiles],
  { cwd: root, encoding: "utf8", stdio: "inherit" },
);
if (prettier.error) throw prettier.error;
process.exitCode = prettier.status ?? 1;

function jsonFiles(directory) {
  return readdirSync(directory, { withFileTypes: true })
    .flatMap((entry) => {
      const path = resolve(directory, entry.name);
      return entry.isDirectory() ? jsonFiles(path) : [path];
    })
    .filter((path) => path.endsWith(".json"))
    .sort();
}
