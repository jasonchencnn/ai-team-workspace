import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { afterEach, describe, expect, it } from "vitest";

import { pathForCommand } from "../../scripts/verify-tooling-guards.mjs";

const temporaryDirectories: string[] = [];

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    rmSync(directory, { force: true, recursive: true });
  }
});

describe("tooling guard command paths", () => {
  it("decodes Unicode and space characters for real command arguments", () => {
    const directory = mkdtempSync(join(tmpdir(), "工具 guard "));
    temporaryDirectories.push(directory);
    const expectedPath = join(directory, "中文目录", "invalid file.ts");
    const encodedUrl = pathToFileURL(expectedPath);

    expect(encodedUrl.pathname).toContain("%E4%B8%AD%E6%96%87");
    expect(encodedUrl.pathname).toContain("%20");
    expect(pathForCommand(encodedUrl)).toBe(expectedPath);
  });
});
