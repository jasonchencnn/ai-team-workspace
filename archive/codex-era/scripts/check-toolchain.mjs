import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const nodeVersion = readFileSync(new URL("../.nvmrc", import.meta.url), "utf8").trim();
const rustVersion = "1.98.0";
const pnpmVersion = "10.33.0";

function requireVersion(name, actual, expected) {
  if (actual !== expected) {
    throw new Error(`${name} must be ${expected}; found ${actual}.`);
  }
}

requireVersion("Node", process.versions.node, nodeVersion);
requireVersion(
  "pnpm",
  execFileSync("pnpm", ["--version"], { encoding: "utf8" }).trim(),
  pnpmVersion,
);

const rustc = execFileSync("rustc", ["--version"], { encoding: "utf8" });
const actualRustVersion = /^rustc (\d+\.\d+\.\d+)/.exec(rustc)?.[1];
requireVersion("Rust", actualRustVersion, rustVersion);

process.stdout.write(
  `Toolchains verified: Node ${nodeVersion}, pnpm ${pnpmVersion}, Rust ${rustVersion}.\n`,
);
