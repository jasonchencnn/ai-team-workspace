import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { findGeneratedContractDrift } from "./contracts-drift.mjs";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const committed = resolve(process.argv[2] ?? resolve(root, "packages/contracts-ts"));
const suppliedGenerated = process.argv[3];
const temporary = suppliedGenerated
  ? resolve(suppliedGenerated)
  : mkdtempSync(resolve(tmpdir(), "contracts-ts-"));

try {
  if (!suppliedGenerated) {
    const generated = spawnSync(
      "node",
      [resolve(root, "scripts/generate-contracts.mjs"), temporary],
      {
        cwd: root,
        encoding: "utf8",
        stdio: "inherit",
      },
    );
    if (generated.status !== 0) process.exit(generated.status ?? 1);
  }

  const drift = findGeneratedContractDrift(committed, temporary);
  if (drift.length > 0) {
    console.error(`Generated contracts are stale: ${drift.join(", ")}`);
    process.exitCode = 1;
  } else {
    console.log("Generated contracts are up to date.");
  }
} finally {
  if (!suppliedGenerated) rmSync(temporary, { recursive: true, force: true });
}
