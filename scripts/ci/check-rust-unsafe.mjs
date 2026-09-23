import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const scanRoots = [resolve(root, "crates"), resolve(root, "apps/desktop/src-tauri")];
const violations = [];

for (const scanRoot of scanRoots) scan(scanRoot);

if (violations.length > 0) {
  process.stderr.write(
    `Rust unsafe policy violations:\n${violations.map((item) => `- ${item}`).join("\n")}\n`,
  );
  process.exit(1);
}

process.stdout.write("Rust unsafe policy passed.\n");

function scan(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) {
      scan(path);
      continue;
    }
    if (!entry.isFile() || !entry.name.endsWith(".rs")) continue;

    const lines = readFileSync(path, "utf8").split(/\r?\n/u);
    for (const [index, line] of lines.entries()) {
      if (/\bunsafe\b/u.test(line) || /allow\s*\(\s*unsafe_code\s*\)/u.test(line)) {
        violations.push(`${path}:${index + 1}: ${line.trim()}`);
      }
    }
  }
}
