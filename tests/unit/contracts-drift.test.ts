import { spawnSync } from "node:child_process";
import { cpSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { findGeneratedContractDrift } from "../../scripts/contracts-drift.mjs";

const temporaryDirectories: string[] = [];
const generatedContracts = resolve("packages/contracts-ts");

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    rmSync(directory, { recursive: true, force: true });
  }
});

describe("generated contract drift", () => {
  it("accepts an identical generated tree", () => {
    const copy = copyGeneratedContracts();
    expect(findGeneratedContractDrift(generatedContracts, copy)).toEqual([]);
  });

  it("rejects a manually edited generated file", () => {
    const copy = copyGeneratedContracts();
    writeFileSync(resolve(copy, "index.ts"), "// manual edit\n", "utf8");
    expect(findGeneratedContractDrift(generatedContracts, copy)).toEqual(["index.ts"]);
  });

  it("returns a non-zero exit code for manually edited generated output", () => {
    const copy = copyGeneratedContracts();
    writeFileSync(resolve(copy, "index.ts"), "// manual edit\n", "utf8");
    const result = spawnSync("node", ["scripts/check-contracts.mjs", generatedContracts, copy], {
      encoding: "utf8",
    });
    expect(result.status).toBe(1);
    expect(result.stderr).toContain("Generated contracts are stale: index.ts");
  });
});

function copyGeneratedContracts() {
  const directory = mkdtempSync(resolve(tmpdir(), "contracts-drift-test-"));
  temporaryDirectories.push(directory);
  cpSync(generatedContracts, directory, { recursive: true });
  return directory;
}
