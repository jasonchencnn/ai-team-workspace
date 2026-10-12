import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, unlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** @param {URL} url */
export function pathForCommand(url) {
  return fileURLToPath(url);
}

function expectFailure(command, args, cwd, expectedOutput) {
  try {
    execFileSync(command, args, { cwd, encoding: "utf8", stdio: "pipe" });
  } catch (error) {
    const output = `${error.stdout ?? ""}\n${error.stderr ?? ""}`;

    if (!expectedOutput.test(output)) {
      throw new Error(
        `${command} ${args.join(" ")} failed without the expected diagnostic ${expectedOutput}:\n${output}`,
        { cause: error },
      );
    }

    return;
  }

  throw new Error(`${command} ${args.join(" ")} unexpectedly succeeded.`);
}

export function main() {
  const temporaryDirectory = mkdtempSync(join(tmpdir(), "ai-team-workspace-tooling-"));
  const invalidTypeScriptUrl = new URL("../tests/tooling-guard.invalid.ts", import.meta.url);
  const invalidTypeScriptPath = pathForCommand(invalidTypeScriptUrl);
  const workspaceDirectory = pathForCommand(new URL("../", import.meta.url));

  try {
    writeFileSync(invalidTypeScriptPath, "const value: any = 1;\nvoid value;\n");
    expectFailure(
      "pnpm",
      ["exec", "eslint", "--no-ignore", invalidTypeScriptPath],
      workspaceDirectory,
      /@typescript-eslint\/no-explicit-any/,
    );

    writeFileSync(
      join(temporaryDirectory, "Cargo.toml"),
      '[package]\nname = "tooling-guard"\nversion = "0.0.0"\nedition = "2024"\n',
    );
    const sourceDirectory = join(temporaryDirectory, "src");
    mkdirSync(sourceDirectory);
    writeFileSync(
      join(temporaryDirectory, "src", "lib.rs"),
      "pub fn guard() { let unused = 1; }\n",
    );
    expectFailure(
      "cargo",
      ["clippy", "--manifest-path", join(temporaryDirectory, "Cargo.toml"), "--", "-D", "warnings"],
      temporaryDirectory,
      /unused variable|unused_variables|warnings being denied/i,
    );
  } finally {
    unlinkSync(invalidTypeScriptPath);
    rmSync(temporaryDirectory, { force: true, recursive: true });
  }

  process.stdout.write("Intentional TypeScript and Rust violations were rejected.\n");
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
