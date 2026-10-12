import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  assertCompleteHistoryScope,
  assertMatchingCompanionEvidence,
  assertMatchingGateInputs,
  assertCompletedGateRun,
  assertMatchingInputManifests,
  buildIgnoredBuildInputManifest,
  buildInputManifest,
  canonicalGateCommandCount,
  collectGateInputs,
  collectInputManifest,
  companionCheckDefinitions,
  companionCheckIds,
  companionLogDigest,
  listIgnoredBuildInputPaths,
  createGatePlan,
  isUbuntuOsRelease,
  normalizeCompanionLog,
  parseArguments,
  requiredToolchains,
  validateRequiredCapture,
  validateToolchains,
} from "./local-gates.mjs";

const ciDirectory = resolve(fileURLToPath(new URL(".", import.meta.url)));

test("parses the macOS dry-run request", () => {
  assert.deepEqual(parseArguments(["--platform", "macos", "--dry-run"]), {
    dryRun: true,
    evidencePath: undefined,
    platform: "macos",
  });
});

test("macOS plan covers every required local gate without a shell", () => {
  const plan = createGatePlan({
    dryRun: false,
    evidencePath: undefined,
    platform: "macos",
  });

  assert.equal(plan.runner, "local-environment");
  assert.deepEqual(plan.commands.slice(0, 3), [
    { command: "gitleaks", args: ["version"] },
    { command: "cargo", args: ["audit", "--version"] },
    { command: "cargo", args: ["deny", "--version"] },
  ]);
  assert.deepEqual(plan.commands[3], {
    command: "gitleaks",
    args: [
      "detect",
      "--source",
      ".",
      "--no-git",
      "--redact",
      "--config",
      ".gitleaks.toml",
      "--exit-code",
      "1",
    ],
  });
  assert.ok(hasCommand(plan, "pnpm", ["install", "--frozen-lockfile"]));
  assert.ok(hasCommand(plan, "pnpm", ["run", "fmt:check"]));
  assert.ok(hasCommand(plan, "pnpm", ["run", "typecheck"]));
  assert.ok(hasCommand(plan, "pnpm", ["run", "test"]));
  assert.ok(hasCommand(plan, "pnpm", ["run", "build"]));
  assert.ok(hasCommand(plan, "cargo", ["test", "--workspace", "--locked"]));
  assert.ok(hasCommand(plan, "cargo", ["build", "--workspace", "--locked"]));
  assert.ok(
    hasCommand(plan, "cargo", [
      "clippy",
      "--workspace",
      "--all-targets",
      "--all-features",
      "--locked",
      "--",
      "-D",
      "warnings",
    ]),
  );
  assert.ok(plan.commands.some(({ command }) => command === "gitleaks"));
  assert.ok(hasCommand(plan, "pnpm", ["audit", "--audit-level=high"]));
  assert.ok(hasCommand(plan, "cargo", ["audit"]));
  assert.ok(
    hasCommand(plan, "cargo", ["deny", "check", "--config", ".github/cargo-deny.toml", "licenses"]),
  );
});

test("Ubuntu ARM64 plan retains every gate without a nested container dependency", () => {
  const plan = createGatePlan({
    dryRun: false,
    evidencePath: "/tmp/dev-01-05.json",
    platform: "ubuntu",
  });

  assert.equal(plan.runner, "local-environment");
  assert.equal(plan.commands.length, createGatePlan({ platform: "macos" }).commands.length);
  assert.ok(!plan.commands.some(({ command }) => command === "podman"));
});

test("rejects unsupported platforms and unsafe evidence destinations", () => {
  assert.throws(() => parseArguments(["--platform", "windows"]), /unsupported platform/u);
  assert.throws(
    () => parseArguments(["--platform", "ubuntu", "--evidence", "proof.json"]),
    /absolute path/u,
  );
});

test("accepts Ubuntu os-release markers without constraining CPU architecture", () => {
  assert.equal(isUbuntuOsRelease(["ID=ubuntu", "VERSION_ID=24.04"]), true);
  assert.equal(isUbuntuOsRelease(['ID="debian"', "VERSION_ID=12"]), false);
});

test("Ubuntu gate image pins tools and drops root before execution", () => {
  const dockerfile = readFileSync(resolve(ciDirectory, "ubuntu-gate.Dockerfile"), "utf8");

  assert.match(
    dockerfile,
    /^FROM ubuntu:24\.04@sha256:008173c23f95b170204355c12626cb5a965d779a7e1283b09e9cffbb1bf33ca3$/mu,
  );

  for (const pin of [
    "NODE_VERSION=24.21.0",
    "PNPM_VERSION=10.33.0",
    "RUST_VERSION=1.98.0",
    "GITLEAKS_VERSION=8.30.0",
    "CARGO_AUDIT_VERSION=0.22.0",
    "CARGO_DENY_VERSION=0.18.6",
    "RUSTUP_INIT_SHA256=7d0ea0f8eba7fa1ebfe998091cd7ec4501e33ec5ca6b884eb4d894d7da5170af",
    "NODE_LINUX_ARM64_SHA256=6ad1325edbdb5649c379b75a237147a666c95d4f9ae8d340fef2d1575d289ad2",
    "GITLEAKS_LINUX_ARM64_SHA256=b4cbbb6ddf7d1b2a603088cd03a4e3f7ce48ee7fd449b51f7de6ee2906f5fa2f",
  ]) {
    assert.ok(dockerfile.includes(pin), `missing tool pin: ${pin}`);
  }
  for (const dependency of [
    "libwebkit2gtk-4.1-dev",
    "libayatana-appindicator3-dev",
    "librsvg2-dev",
    "libxdo-dev",
    "patchelf",
  ]) {
    assert.ok(dockerfile.includes(dependency), `missing Ubuntu dependency: ${dependency}`);
  }
  assert.match(dockerfile, /test "\$\(id -u ubuntu\)" = "1000"/u);
  assert.ok(dockerfile.includes("CARGO_BUILD_JOBS=2"));
  assert.equal((dockerfile.match(/sha256sum --check --strict/gu) ?? []).length, 3);
  assert.match(dockerfile, /USER ubuntu\nWORKDIR \/workspace/u);
  assert.ok(!dockerfile.includes("readest"));
  assert.ok(!dockerfile.includes("docker.sock"));
});

test("Gitleaks policy excludes generated trees and keeps fixture exceptions narrow", () => {
  const policy = readFileSync(resolve(ciDirectory, "../../.gitleaks.toml"), "utf8");
  const generatedPaths = new Set([
    String.raw`(^|/)(node_modules|target|\.pnpm-store|dist)(/|$)`,
    String.raw`(^|/)\._[^/]+$`,
  ]);

  function literalArray(block, name) {
    const contents = block.match(new RegExp(`^${name}\\s*=\\s*\\[([\\s\\S]*?)\\]\\s*$`, "mu"));
    assert.ok(contents, `${name} must be an explicit array`);
    return [...contents[1].matchAll(/'''([\s\S]*?)'''/gu)].map((match) => match[1]);
  }

  function assertNarrowPolicy(candidate) {
    assert.match(candidate, /^useDefault = true$/mu);
    const blocks = candidate.split(/(?=^\[\[)/mu);
    const global = blocks.filter((block) => block.startsWith("[[allowlists]]"));
    const scoped = blocks.filter((block) => block.startsWith("[[rules.allowlists]]"));
    const exemptedPaths = new Set();
    for (const block of global) {
      assert.ok(!block.includes("regexes"), "content exceptions must be rule-level");
      for (const path of literalArray(block, "paths")) {
        assert.ok(generatedPaths.has(path), `global file exemption is unsafe: ${path}`);
        exemptedPaths.add(path);
      }
    }
    assert.deepEqual(exemptedPaths, generatedPaths, "generated trees remain excluded");
    assert.ok(scoped.some((block) => block.includes("design/figma-state")));
    assert.ok(
      scoped.some((block) => block.includes("crates/observability/tests/structured_logging")),
    );
    for (const block of scoped) {
      assert.match(block, /^condition = "AND"$/mu);
      assert.match(block, /^regexTarget = "(?:line|secret)"$/mu);
      const paths = literalArray(block, "paths");
      const patterns = literalArray(block, "regexes");
      assert.ok(paths.length > 0 && patterns.length > 0);
      assert.ok(paths.every((path) => path.startsWith("(^|/)") && path.endsWith("$")));
      assert.ok(patterns.every((pattern) => pattern.startsWith("^") && pattern.endsWith("$")));
      assert.ok(!block.includes(".*"), "fixture exceptions cannot accept arbitrary contents");
    }
  }

  assertNarrowPolicy(policy);
  assert.throws(
    () => assertNarrowPolicy(policy.replaceAll("[[rules.allowlists]]", "[[allowlists]]")),
    /content exceptions must be rule-level|global file exemption is unsafe/u,
  );
  assert.throws(() =>
    assertNarrowPolicy(policy.replaceAll('condition = "AND"', 'condition = "OR"')),
  );
  assert.throws(() => assertNarrowPolicy(policy.replaceAll("(^|/)design/figma-state", ".*")));
});

test("buildInputManifest sorts UTF-8 paths and makes its total hash reproducible", () => {
  const first = buildInputManifest([
    { bytes: Buffer.from("two\n", "utf8"), path: "z/second.txt" },
    { bytes: Buffer.from("one\n", "utf8"), path: "a/first.txt" },
  ]);
  const second = buildInputManifest([
    { bytes: Buffer.from("one\n", "utf8"), path: "a/first.txt" },
    { bytes: Buffer.from("two\n", "utf8"), path: "z/second.txt" },
  ]);

  assert.equal(first.encoding, "utf-8");
  assert.equal(first.format, "ai-team-input-manifest/v2");
  assert.deepEqual(first.skipped_tracked_deleted_paths, []);
  assert.deepEqual(
    first.entries.map((entry) => entry.path),
    ["a/first.txt", "z/second.txt"],
  );
  assert.match(first.total_sha256, /^[a-f0-9]{64}$/u);
  assert.equal(first.total_sha256, second.total_sha256);
  assert.equal(first.entries[0].mode, "100644");
});

test("input manifest identity includes file mode and must remain stable after gates", () => {
  const before = buildInputManifest([
    { bytes: Buffer.from("fixture\n", "utf8"), mode: "100644", path: "script.sh" },
  ]);
  const executable = buildInputManifest([
    { bytes: Buffer.from("fixture\n", "utf8"), mode: "100755", path: "script.sh" },
  ]);

  assert.notEqual(before.total_sha256, executable.total_sha256);
  assert.throws(
    () => assertMatchingInputManifests(before, executable),
    /input manifest changed while gates were running/u,
  );
  assert.doesNotThrow(() => assertMatchingInputManifests(before, structuredClone(before)));
});

test("collectInputManifest skips only explicitly tracked deleted paths and records them", () => {
  const safeStat = { isFile: () => true, isSymbolicLink: () => false };
  const inspectedPaths = [];
  const manifest = collectInputManifest({
    dependencies: {
      listPaths: () => [
        { deleted: true, path: "legacy/source.md", tracked: true },
        { path: "docs/archive/source.md", tracked: false },
      ],
      lstat: (path) => {
        inspectedPaths.push(path);
        return safeStat;
      },
      readFile: () => Buffer.from("archived source", "utf8"),
    },
    repoRoot: "/workspace",
  });

  assert.deepEqual(
    manifest.entries.map((entry) => entry.path),
    ["docs/archive/source.md"],
  );
  assert.deepEqual(manifest.skipped_tracked_deleted_paths, ["legacy/source.md"]);
  assert.equal(inspectedPaths.length, 1);

  assert.throws(
    () =>
      collectInputManifest({
        dependencies: {
          listPaths: () => [{ deleted: true, path: "untracked-missing", tracked: false }],
          lstat: () => {
            throw new Error("ENOENT");
          },
          readFile: () => Buffer.from("unused", "utf8"),
        },
        repoRoot: "/workspace",
      }),
    /could not inspect input untracked-missing: ENOENT/u,
  );
});

test("collectInputManifest excludes outputs and fails closed for unsafe file inputs", () => {
  const safeStat = { isFile: () => true, isSymbolicLink: () => false };
  const baseDependencies = {
    lstat: () => safeStat,
    readFile: () => Buffer.from("fixture", "utf8"),
  };
  const manifest = collectInputManifest({
    dependencies: {
      ...baseDependencies,
      listPaths: () => [
        ".git/config",
        "dist/app.js",
        "node_modules/example/index.js",
        "target/debug/output",
        ".pnpm-store/cache",
        "._FinderInfo",
        "included.txt",
        "evidence.json",
      ],
    },
    evidencePath: "/workspace/evidence.json",
    repoRoot: "/workspace",
  });
  assert.deepEqual(
    manifest.entries.map((entry) => entry.path),
    ["included.txt"],
  );

  assert.throws(
    () =>
      collectInputManifest({
        dependencies: {
          ...baseDependencies,
          listPaths: () => ["link"],
          lstat: () => ({ isFile: () => false, isSymbolicLink: () => true }),
        },
        repoRoot: "/workspace",
      }),
    /symbolic link/u,
  );
  assert.throws(
    () =>
      collectInputManifest({
        dependencies: {
          ...baseDependencies,
          listPaths: () => ["directory"],
          lstat: () => ({ isFile: () => false, isSymbolicLink: () => false }),
        },
        repoRoot: "/workspace",
      }),
    /non-regular file/u,
  );
  assert.throws(
    () =>
      collectInputManifest({
        dependencies: {
          ...baseDependencies,
          listPaths: () => ["unreadable"],
          readFile: () => {
            throw new Error("permission denied");
          },
        },
        repoRoot: "/workspace",
      }),
    /could not read input/u,
  );
  assert.throws(
    () =>
      collectInputManifest({
        dependencies: { ...baseDependencies, listPaths: () => ["../escape"] },
        repoRoot: "/workspace",
      }),
    /normalized relative path/u,
  );
  assert.throws(
    () =>
      collectInputManifest({
        dependencies: {
          ...baseDependencies,
          listPaths: () => {
            throw new Error("git unavailable");
          },
        },
        repoRoot: "/workspace",
      }),
    /could not list input files/u,
  );
  assert.throws(
    () =>
      collectInputManifest({
        dependencies: {
          ...baseDependencies,
          listPaths: () => ["metadata-failure"],
          lstat: () => {
            throw new Error("I/O error");
          },
        },
        repoRoot: "/workspace",
      }),
    /could not inspect input/u,
  );
  assert.throws(
    () =>
      buildInputManifest([
        { bytes: Buffer.from("same"), path: "duplicate" },
        { bytes: Buffer.from("same"), path: "duplicate" },
      ]),
    /duplicate path/u,
  );
});

test("ignored build inputs record only path, mode, size, and digest; never a value", () => {
  const manifest = buildIgnoredBuildInputManifest([
    { bytes: Buffer.from("VITE_ONE=one\n", "utf8"), path: "z/.env.production" },
    { bytes: Buffer.from("VITE_TWO=two\n", "utf8"), mode: "100755", path: "a/.env.local" },
  ]);

  assert.equal(manifest.format, "ai-team-ignored-build-inputs/v1");
  assert.deepEqual(
    manifest.entries.map((entry) => entry.path),
    ["a/.env.local", "z/.env.production"],
  );
  assert.match(manifest.total_sha256, /^[a-f0-9]{64}$/u);
  assert.deepEqual(Object.keys(manifest.entries[0]), ["mode", "path", "sha256", "size_bytes"]);
  // The gate may not carry the value itself, or any prefix/suffix of it.
  assert.ok(!JSON.stringify(manifest).includes("VITE_ONE"));
  assert.ok(!JSON.stringify(manifest).includes("one\n"));

  // Same bytes at the same path reproduce the same digest; different bytes do not.
  assert.equal(
    buildIgnoredBuildInputManifest([
      { bytes: Buffer.from("VITE_ONE=one\n", "utf8"), path: "z/.env.production" },
    ]).total_sha256,
    buildIgnoredBuildInputManifest([
      { bytes: Buffer.from("VITE_ONE=one\n", "utf8"), path: "z/.env.production" },
    ]).total_sha256,
  );
  assert.notEqual(
    buildIgnoredBuildInputManifest([
      { bytes: Buffer.from("VITE_ONE=one\n", "utf8"), path: "z/.env.production" },
    ]).total_sha256,
    buildIgnoredBuildInputManifest([
      { bytes: Buffer.from("VITE_ONE=other\n", "utf8"), path: "z/.env.production" },
    ]).total_sha256,
  );
  assert.notEqual(
    buildIgnoredBuildInputManifest([{ bytes: Buffer.from("same", "utf8"), path: ".env" }])
      .total_sha256,
    buildIgnoredBuildInputManifest([
      { bytes: Buffer.from("same", "utf8"), mode: "100755", path: ".env" },
    ]).total_sha256,
  );

  assert.throws(
    () =>
      buildIgnoredBuildInputManifest([
        { bytes: Buffer.from("same", "utf8"), path: ".env" },
        { bytes: Buffer.from("same", "utf8"), path: ".env" },
      ]),
    /duplicate path/u,
  );
  assert.throws(
    () =>
      buildIgnoredBuildInputManifest([{ bytes: Buffer.from("x", "utf8"), path: "../escape.env" }]),
    /normalized relative path/u,
  );
  assert.throws(
    () => buildIgnoredBuildInputManifest([{ bytes: "not bytes", path: ".env" }]),
    /was not read as bytes/u,
  );
  assert.throws(
    () =>
      buildIgnoredBuildInputManifest([
        { bytes: Buffer.from("x", "utf8"), mode: "120000", path: ".env" },
      ]),
    /unsupported mode/u,
  );
});

test("the ignored build input listing skips dependency caches, outputs, and dotenv samples", () => {
  const fixture = createIsolatedGitFixture();
  try {
    writeFileSync(join(fixture, ".gitignore"), "node_modules/\n.env\n.env.*\n!.env.example\n");
    writeFileSync(join(fixture, ".env.example"), "VITE_SAMPLE=0\n");
    writeFileSync(join(fixture, ".env.production"), "VITE_SAMPLE=1\n");
    writeFileSync(join(fixture, "apps.local"), "x\n");
    for (const relative of [
      "node_modules/vite/.env.cache",
      "dist/.env.bundle",
      "target/.env.out",
    ]) {
      mkdirSync(join(fixture, relative, ".."), { recursive: true });
      writeFileSync(join(fixture, relative), "VITE_SAMPLE=2\n");
    }
    // Nested and untracked-directory cases must both be reported: an ignored
    // dotenv inside an untracked folder is exactly the DA-06 blind spot.
    mkdirSync(join(fixture, "untracked/nested"), { recursive: true });
    writeFileSync(join(fixture, "untracked/nested/.env"), "VITE_SAMPLE=3\n");
    writeFileSync(join(fixture, "untracked/nested/.env.example"), "VITE_SAMPLE=4\n");
    assert.deepEqual(listIgnoredBuildInputPaths(fixture), [
      ".env.production",
      "untracked/nested/.env",
    ]);
  } finally {
    rmSync(fixture, { force: true, recursive: true });
  }
});

test("gate input identity binds ignored build inputs so different .env content cannot share a hash", () => {
  const fixture = createIsolatedGitFixture();
  try {
    writeFileSync(join(fixture, ".gitignore"), ".env\n.env.*\n");
    writeFileSync(join(fixture, "package.json"), ' {"private":true}\n');
    const before = collectGateInputs({ repoRoot: fixture });

    writeFileSync(join(fixture, ".env.production"), "NODE_ENV=development\nVITE_AUDIT_LABEL=one\n");
    const first = collectGateInputs({ repoRoot: fixture });
    writeFileSync(join(fixture, ".env.production"), "NODE_ENV=production\nVITE_AUDIT_LABEL=two\n");
    const second = collectGateInputs({ repoRoot: fixture });

    // The tracked/untracked manifest stays identical, which is exactly the
    // blind spot: two different external build inputs, one manifest hash.
    assert.equal(first.input_manifest.total_sha256, second.input_manifest.total_sha256);
    assert.notEqual(before.total_sha256, first.total_sha256);
    assert.notEqual(first.total_sha256, second.total_sha256);
    assert.deepEqual(
      first.ignored_build_inputs.entries.map((entry) => entry.path),
      [".env.production"],
    );
    assert.throws(
      () =>
        assertMatchingGateInputs(
          { total_sha256: first.total_sha256 },
          { total_sha256: second.total_sha256 },
        ),
      /gate inputs changed while gates were running/u,
    );
    assert.doesNotThrow(() =>
      assertMatchingGateInputs({ total_sha256: first.total_sha256 }, first),
    );
  } finally {
    rmSync(fixture, { force: true, recursive: true });
  }
});

test("a legal control without ignored build inputs keeps one stable gate input identity", () => {
  const fixture = createIsolatedGitFixture();
  try {
    writeFileSync(join(fixture, ".gitignore"), "node_modules/\n");
    writeFileSync(join(fixture, "package.json"), ' {"private":true}\n');
    writeFileSync(join(fixture, ".env.example"), "VITE_SAMPLE=0\n");
    const first = collectGateInputs({ repoRoot: fixture });
    const second = collectGateInputs({ repoRoot: fixture });

    assert.deepEqual(first.ignored_build_inputs.entries, []);
    assert.equal(first.total_sha256, second.total_sha256);
    assert.equal(first.input_manifest.total_sha256, second.input_manifest.total_sha256);
    assert.ok(
      first.input_manifest.entries.some((entry) => entry.path === ".env.example"),
      "an example file that is not ignored stays a normal tracked-class input",
    );
  } finally {
    rmSync(fixture, { force: true, recursive: true });
  }
});

test("ignored build inputs are held to the same path, link, and failure rules", () => {
  const safeStat = { isFile: () => true, isSymbolicLink: () => false, mode: 0o100644 };
  const baseDependencies = {
    listPaths: () => [],
    lstat: () => safeStat,
    readFile: () => Buffer.from("VITE_SAMPLE=0\n", "utf8"),
  };

  assert.throws(
    () =>
      collectGateInputs({
        dependencies: { ...baseDependencies, listIgnoredPaths: () => ["../outside.env"] },
        repoRoot: "/workspace",
      }),
    /normalized relative path/u,
  );
  assert.throws(
    () =>
      collectGateInputs({
        dependencies: {
          ...baseDependencies,
          listIgnoredPaths: () => [".env.production"],
          lstat: () => ({ isFile: () => true, isSymbolicLink: () => true, mode: 0o120777 }),
        },
        repoRoot: "/workspace",
      }),
    /symbolic link/u,
  );
  assert.throws(
    () =>
      collectGateInputs({
        dependencies: {
          ...baseDependencies,
          listIgnoredPaths: () => [".env.production"],
          lstat: () => ({ isFile: () => false, isSymbolicLink: () => false, mode: 0o040755 }),
        },
        repoRoot: "/workspace",
      }),
    /non-regular file/u,
  );
  assert.throws(
    () =>
      collectGateInputs({
        dependencies: {
          ...baseDependencies,
          listIgnoredPaths: () => [".env.production"],
          lstat: () => {
            throw new Error("EACCES");
          },
        },
        repoRoot: "/workspace",
      }),
    /could not inspect ignored build input/u,
  );
  assert.throws(
    () =>
      collectGateInputs({
        dependencies: {
          ...baseDependencies,
          listIgnoredPaths: () => [".env.production"],
          readFile: () => {
            throw new Error("permission denied");
          },
        },
        repoRoot: "/workspace",
      }),
    /could not read ignored build input/u,
  );
  assert.throws(
    () =>
      collectGateInputs({
        dependencies: {
          ...baseDependencies,
          listIgnoredPaths: () => {
            throw new Error("git unavailable");
          },
        },
        repoRoot: "/workspace",
      }),
    /could not list ignored build inputs/u,
  );
  assert.throws(
    () =>
      collectGateInputs({
        dependencies: { ...baseDependencies, listIgnoredPaths: () => ".env" },
        repoRoot: "/workspace",
      }),
    /not parseable/u,
  );
});

test("history scope evidence must prove a non-shallow repository with refs", () => {
  const complete = {
    is_shallow: "false",
    refs: ["a".repeat(40) + " refs/heads/main"],
    refs_sha256: "b".repeat(64),
  };

  assert.equal(assertCompleteHistoryScope(complete), complete);
  assert.throws(
    () => assertCompleteHistoryScope({ ...complete, is_shallow: "true" }),
    /non-shallow repository/u,
  );
  assert.throws(() => assertCompleteHistoryScope({ ...complete, refs: [] }), /recorded no refs/u);
  assert.throws(() => assertCompleteHistoryScope({ ...complete, refs_sha256: "" }), /refs digest/u);
  assert.throws(() => assertCompleteHistoryScope(undefined), /non-shallow repository/u);
});

test("companion log digests ignore timings and colour but keep decision-bearing text", () => {
  const scanLog = (timing, size, verdict) =>
    `[90m2:09PM[0m [32mINF[0m 4 commits scanned.\n` +
    `[90m2:09PM[0m [32mINF[0m scanned ~${size} bytes (${size} KB) in ${timing}\n` +
    `[90m2:09PM[0m [32mINF[0m ${verdict}`;

  // Two runs of the same clean scan differ only in timing and colour.
  assert.equal(
    companionLogDigest(scanLog("350ms", 709699, "no leaks found")),
    companionLogDigest(scanLog("1.2s", 709699, "no leaks found")),
  );
  // A different decision must still change the digest.
  assert.notEqual(
    companionLogDigest(scanLog("350ms", 709699, "no leaks found")),
    companionLogDigest(scanLog("350ms", 709699, "1 leaks found")),
  );
  assert.notEqual(
    companionLogDigest(scanLog("350ms", 709699, "no leaks found")),
    companionLogDigest(scanLog("350ms", 709700, "no leaks found")),
  );
  // A real change in the scanned bytes must change the digest too.
  assert.notEqual(
    companionLogDigest(scanLog("350ms", 709699, "no leaks found")),
    companionLogDigest(
      scanLog("350ms", 709699, "no leaks found").replace("4 commits", "5 commits"),
    ),
  );
});

test("companion log prefixes ignore Gitleaks clock changes without hiding result changes", () => {
  const scanLog = (clock) =>
    `\u001b[90m${clock}\u001b[0m INF 4 commits scanned.\n` +
    `${clock} INF scanned ~709699 bytes (709.70 KB) in 350ms\n` +
    `${clock} INF no leaks found\n`;
  const first = scanLog("1:22PM");
  for (const clock of ["1:25PM", "9:09AM", "12:00AM", "12:00PM"]) {
    assert.equal(companionLogDigest(first), companionLogDigest(scanLog(clock)));
  }
  for (const changed of [
    first.replace("no leaks found", "1 leaks found"),
    first.replace("4 commits", "5 commits"),
    first.replace("709699 bytes", "709700 bytes"),
    first.replace("INF no leaks", "WRN no leaks"),
  ]) {
    assert.notEqual(companionLogDigest(first), companionLogDigest(changed));
  }
  // A clock inside payload text, or without a logger level, is still data.
  for (const suffix of ["payload clock: ", ""]) {
    assert.notEqual(
      companionLogDigest(`${first}${suffix}1:22PM message\n`),
      companionLogDigest(`${first}${suffix}1:25PM message\n`),
    );
  }
});

test("companion evidence blocks pass when it is missing, failing, undeclared, or drifting", () => {
  const passing = companionCheckDefinitions().map((definition) => ({
    ...definition,
    exit_code: 0,
    history_scope: { refs_sha256: "c".repeat(64) },
    log_sha256: "d".repeat(64),
  }));

  assert.doesNotThrow(() => assertMatchingCompanionEvidence(passing, structuredClone(passing)));
  assert.deepEqual(
    createGatePlan({ platform: "macos" }).companion_checks.map((check) => check.id),
    [...companionCheckIds],
  );

  assert.throws(
    () => assertMatchingCompanionEvidence(passing.slice(1), structuredClone(passing)),
    /companion check evidence is missing: full-refs-history-secret-scan/u,
  );
  assert.throws(
    () =>
      assertMatchingCompanionEvidence(
        passing,
        structuredClone(passing).map((record) =>
          record.id === "tooling-guards" ? { ...record, exit_code: 1 } : record,
        ),
      ),
    /companion check failed: tooling-guards/u,
  );
  assert.throws(
    () =>
      assertMatchingCompanionEvidence(
        passing,
        structuredClone(passing).map((record) =>
          record.id === "tooling-guards" ? { ...record, log_sha256: "f".repeat(64) } : record,
        ),
      ),
    /companion check log digest drifted: tooling-guards/u,
  );
  assert.throws(
    () =>
      assertMatchingCompanionEvidence(
        passing,
        structuredClone(passing).map((record) =>
          record.id === "full-refs-history-secret-scan"
            ? { ...record, history_scope: { refs_sha256: "0".repeat(64) } }
            : record,
        ),
      ),
    /history scope drifted: full-refs-history-secret-scan/u,
  );
  assert.throws(
    () =>
      assertMatchingCompanionEvidence(
        [...passing, { exit_code: 0, id: "extra-check", log_sha256: "a".repeat(64) }],
        structuredClone(passing),
      ),
    /undeclared check: extra-check/u,
  );
});

test("an overall pass needs the full canonical count, a stable gate identity, and complete companions", () => {
  const plan = createGatePlan({ platform: "macos" });
  const fixture = createIsolatedGitFixture();
  try {
    writeFileSync(join(fixture, ".gitignore"), ".env\n.env.*\n");
    writeFileSync(join(fixture, "package.json"), ' {"private":true}\n');
    writeFileSync(join(fixture, ".env.production"), "VITE_SAMPLE=before\n");
    const initial = collectGateInputs({ repoRoot: fixture });
    const companion = companionCheckDefinitions().map((definition) => ({
      ...definition,
      exit_code: 0,
      history_scope: { refs_sha256: "1".repeat(64) },
      log_sha256: "2".repeat(64),
    }));
    const unchanged = initial;
    const summary = assertCompletedGateRun({
      companionAfter: structuredClone(companion),
      companionBefore: structuredClone(companion),
      executedCommandCount: plan.canonical_command_count,
      finalGateInputs: unchanged,
      initialGateIdentity: initial,
      plan,
    });

    assert.equal(summary.canonical_commands_executed, 21);
    assert.equal(summary.gate_input_identity_verified_after_commands, initial.total_sha256);
    assert.equal(
      summary.ignored_build_inputs_verified_after_commands,
      initial.ignored_build_inputs.total_sha256,
    );

    // A shortened canonical list must not be reported as a pass.
    assert.throws(
      () =>
        assertCompletedGateRun({
          companionAfter: structuredClone(companion),
          companionBefore: structuredClone(companion),
          executedCommandCount: 20,
          finalGateInputs: unchanged,
          initialGateIdentity: initial,
          plan,
        }),
      /canonical gate must execute 21 commands; executed 20/u,
    );
    // A changed ignored build input must not be reported as a pass.
    writeFileSync(join(fixture, ".env.production"), "VITE_SAMPLE=after\n");
    assert.throws(
      () =>
        assertCompletedGateRun({
          companionAfter: structuredClone(companion),
          companionBefore: structuredClone(companion),
          executedCommandCount: plan.canonical_command_count,
          finalGateInputs: collectGateInputs({ repoRoot: fixture }),
          initialGateIdentity: initial,
          plan,
        }),
      /gate inputs changed while gates were running/u,
    );
    // Missing identity evidence, missing companions, and failing companions all block.
    assert.throws(
      () =>
        assertCompletedGateRun({
          companionAfter: structuredClone(companion),
          companionBefore: structuredClone(companion),
          executedCommandCount: plan.canonical_command_count,
          finalGateInputs: unchanged,
          initialGateIdentity: undefined,
          plan,
        }),
      /gate input identity evidence is missing/u,
    );
    assert.throws(
      () =>
        assertCompletedGateRun({
          companionAfter: [],
          companionBefore: [],
          executedCommandCount: plan.canonical_command_count,
          finalGateInputs: unchanged,
          initialGateIdentity: initial,
          plan,
        }),
      /companion check evidence is missing/u,
    );
    assert.throws(
      () =>
        assertCompletedGateRun({
          companionAfter: structuredClone(companion).map((record) => ({ ...record, exit_code: 2 })),
          companionBefore: structuredClone(companion),
          executedCommandCount: plan.canonical_command_count,
          finalGateInputs: unchanged,
          initialGateIdentity: initial,
          plan,
        }),
      /companion check failed/u,
    );
  } finally {
    rmSync(fixture, { force: true, recursive: true });
  }
});

test("the canonical gate keeps its fixed command count with companions recorded separately", () => {
  const plan = createGatePlan({ platform: "macos" });
  const source = readFileSync(resolve(ciDirectory, "local-gates.mjs"), "utf8");

  assert.equal(plan.canonical_command_count, 21);
  assert.equal(canonicalGateCommandCount, 21);
  assert.equal(plan.commands.length, canonicalGateCommandCount);
  // The count must be an independent literal, not a value derived from the list.
  assert.match(source, /canonicalGateCommandCount = 21;/u);
  assert.equal((source.match(/const gateCommands = \[/gu) ?? []).length, 1);
  assert.deepEqual(
    plan.companion_checks.map((check) => check.id),
    [...companionCheckIds],
  );
  assert.ok(
    !plan.commands.some(
      ({ command, args }) =>
        command === "gitleaks" && args.includes("git") && args.includes("--log-opts=--all"),
    ),
    "the full history scan must stay outside the canonical command list",
  );
  assert.ok(!plan.commands.some(({ args }) => args.includes("verify:guards")));
  assert.ok(
    hasCommand(plan, "gitleaks", [
      "detect",
      "--source",
      ".",
      "--no-git",
      "--redact",
      "--config",
      ".gitleaks.toml",
      "--exit-code",
      "1",
    ]),
  );
  assert.ok(
    plan.companion_checks.some(
      (check) =>
        check.id === "full-refs-history-secret-scan" &&
        check.args.includes("--log-opts=--all") &&
        check.kind === "history-secret-scan",
    ),
  );
});

test("identity and toolchain evidence reject failed commands, missing tools, and version drift", () => {
  assert.throws(
    () => validateRequiredCapture({ command: "git", exit_code: 1, output: "" }, "git HEAD"),
    /git HEAD failed/u,
  );
  assert.throws(
    () => validateRequiredCapture({ command: "git", exit_code: 0, output: "" }, "git HEAD"),
    /git HEAD produced no parseable output/u,
  );

  const captures = Object.fromEntries(
    Object.entries(requiredToolchains).map(([name, definition]) => [
      name,
      { command: definition.command, exit_code: 0, output: definition.expectedOutput },
    ]),
  );
  assert.doesNotThrow(() => validateToolchains(captures));
  assert.doesNotThrow(() =>
    validateToolchains({
      ...captures,
      cargo_audit: { ...captures.cargo_audit, output: "cargo-audit-audit 0.22.0" },
    }),
  );
  assert.throws(
    () => validateToolchains({ ...captures, pnpm: { ...captures.pnpm, output: "10.33.1" } }),
    /pnpm version mismatch/u,
  );
  assert.throws(
    () => validateToolchains({ ...captures, gitleaks: undefined }),
    /gitleaks did not run/u,
  );
});

test("companion evidence binds each declared id to one record matching its definition", () => {
  // This is an independent verification contract, not a claim that the current
  // main gate is bypassable: records are passed in by the caller, and main()
  // builds them in-process rather than reading an external evidence JSON.
  const complete = () =>
    companionCheckDefinitions().map((definition) => ({
      ...definition,
      exit_code: 0,
      history_scope: { refs_sha256: "a".repeat(64) },
      log_sha256: "b".repeat(64),
    }));

  assert.doesNotThrow(() => assertMatchingCompanionEvidence(complete(), complete()));
  // The definitions the records must match are the ones the plan actually runs.
  assert.deepEqual(
    createGatePlan({ platform: "macos" }).companion_checks,
    companionCheckDefinitions(),
  );

  // A declared id may appear exactly once per side; an extra failed record on
  // the same id must not be selectable as "the" record for that id.
  for (const side of ["before", "after"]) {
    const withExtra = complete();
    withExtra.push({ ...withExtra[0], exit_code: 1 });
    assert.throws(
      () =>
        assertMatchingCompanionEvidence(
          side === "before" ? withExtra : complete(),
          side === "after" ? withExtra : complete(),
        ),
      /records for a declared id/u,
    );
  }

  // Swapping the scan command, its args, or its kind cannot pass as the
  // declared check even when the exit code and digests are otherwise clean.
  const forgeries = [
    { args: ["detect", "--source", "."] },
    { command: "grep" },
    { kind: "command" },
    { args: undefined },
    { args: ["git", "--log-opts=--all", "--config", ".gitleaks.toml"] },
  ];
  for (const forgery of forgeries) {
    const forged = complete().map((record) =>
      record.id === "full-refs-history-secret-scan" ? { ...record, ...forgery } : record,
    );
    assert.throws(
      () => assertMatchingCompanionEvidence(forged, complete()),
      /not match its definition/u,
    );
  }
  const swappedGuards = complete().map((record) =>
    record.id === "tooling-guards" ? { ...record, args: ["run", "test"] } : record,
  );
  assert.throws(
    () => assertMatchingCompanionEvidence(swappedGuards, complete()),
    /not match its definition/u,
  );
});

test("companion log normalisation strips noise but keeps verdicts and scope text", () => {
  const log =
    "[90m2:09PM[0m INF 4 commits scanned in 350ms\n" +
    "[90m2:09PM[0m INF scanned 709699KB\n" +
    "[90m2:09PM[0m INF no leaks found";
  const normalized = normalizeCompanionLog(log);

  // Timing, size, and colour are noise; the verdict and the scanned scope are
  // the decision and must survive into the digest.
  assert.doesNotMatch(normalized, /350ms/u);
  assert.doesNotMatch(normalized, /709699KB/u);
  // The escape byte itself must be gone; the visible log text stays intact.
  assert.doesNotMatch(normalized, new RegExp(String.fromCharCode(27), "u"));
  assert.match(normalized, /no leaks found/u);
  assert.match(normalized, /4 commits scanned/u);
  assert.notEqual(
    normalizeCompanionLog(log),
    normalizeCompanionLog(log.replace("no leaks found", "3 leaks found")),
  );
  assert.notEqual(
    normalizeCompanionLog(log),
    normalizeCompanionLog(log.replace("4 commits", "5 commits")),
  );
});

test("the CI history scan fails closed on a shallow clone before scanning", () => {
  const workflow = readFileSync(resolve(ciDirectory, "../../.github/workflows/ci.yml"), "utf8");
  const check = spawnSync(process.execPath, [resolve(ciDirectory, "verify-workflows.mjs")], {
    encoding: "utf8",
  });

  // The real workflow satisfies the fail-closed contract.
  assert.equal(check.status, 0, `${check.stdout ?? ""}${check.stderr ?? ""}`);

  // The contract is behavioural: the guard step is extracted and executed
  // against a stubbed git, not matched as a printed line.
  assert.match(
    workflow,
    /- name: fail closed unless the clone is complete\n {8}run: \|\n[\s\S]*?exit 1/u,
    "the workflow must fail closed rather than only print the shallow state",
  );
  assert.match(workflow, /!=\s*"false"/u);
  assert.match(workflow, /- run: gitleaks git --log-opts=--all/u);
  // A job that only prints the shallow state is exactly the audited defect.
  assert.doesNotMatch(
    workflow,
    /- run: git rev-parse --is-shallow-repository\s*$/mu,
    "the shallow state must be asserted, not just printed",
  );
});

function hasCommand(plan, command, args) {
  return plan.commands.some(
    (candidate) =>
      candidate.command === command && candidate.args.join("\u0000") === args.join("\u0000"),
  );
}

// Isolated no-commit repository, mirroring the audit fixture that reproduced DA-06.
// Nothing outside this system temp directory is created, read, or removed.
function createIsolatedGitFixture() {
  const directory = mkdtempSync(join(tmpdir(), "mm-r06-gate-input-"));
  execFileSync("git", ["init", "--quiet", "--template=", directory], { cwd: directory });
  return directory;
}
