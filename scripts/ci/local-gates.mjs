import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstatSync, readFileSync, writeFileSync } from "node:fs";
import { isAbsolute, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const inputManifestFormat = "ai-team-input-manifest/v2";
const ignoredBuildInputFormat = "ai-team-ignored-build-inputs/v1";
const gateInputIdentityFormat = "ai-team-gate-input-identity/v1";

// ADR-0005 keeps the canonical gate at a fixed command count. The checks below
// are recorded separately and are required for an overall pass; they never
// replace or reorder a canonical command.
export const companionCheckIds = Object.freeze(["full-refs-history-secret-scan", "tooling-guards"]);

// Build tools read dotenv files that are normally gitignored, so they are
// invisible to `git ls-files --others --exclude-standard`. Names are matched
// only; values are never read into logs or evidence.
const ignoredBuildInputDirectories = new Set([
  ".git",
  "dist",
  "node_modules",
  "target",
  ".pnpm-store",
]);
const ignoredBuildInputNameExemptions = new Set([".env.example", ".env.sample", ".env.template"]);
const ignoredBuildInputBasenamePattern = /^\.env/iu;

export const requiredToolchains = Object.freeze({
  node: {
    args: ["--version"],
    command: "node",
    expectedOutput: "v24.21.0",
    expectedVersion: "24.21.0",
    versionPattern: /^v(?<version>\d+\.\d+\.\d+)$/u,
  },
  pnpm: {
    args: ["--version"],
    command: "pnpm",
    expectedOutput: "10.33.0",
    expectedVersion: "10.33.0",
    versionPattern: /^(?<version>\d+\.\d+\.\d+)$/u,
  },
  rustc: {
    args: ["--version"],
    command: "rustc",
    expectedOutput: "rustc 1.98.0",
    expectedVersion: "1.98.0",
    versionPattern: /^rustc (?<version>\d+\.\d+\.\d+)(?: \([^\r\n]+\))?$/u,
  },
  cargo: {
    args: ["--version"],
    command: "cargo",
    expectedOutput: "cargo 1.98.0",
    expectedVersion: "1.98.0",
    versionPattern: /^cargo (?<version>\d+\.\d+\.\d+)(?: \([^\r\n]+\))?$/u,
  },
  gitleaks: {
    args: ["version"],
    command: "gitleaks",
    expectedOutput: "8.30.0",
    expectedVersion: "8.30.0",
    versionPattern: /(?:^|\s)(?<version>\d+\.\d+\.\d+)(?:\s|$)/u,
  },
  cargo_audit: {
    args: ["audit", "--version"],
    command: "cargo",
    expectedOutput: "cargo-audit-audit 0.22.0",
    expectedVersion: "0.22.0",
    versionPattern: /^cargo-audit(?:-audit)? (?<version>\d+\.\d+\.\d+)$/u,
  },
  cargo_deny: {
    args: ["deny", "--version"],
    command: "cargo",
    expectedOutput: "cargo-deny 0.18.6",
    expectedVersion: "0.18.6",
    versionPattern: /^cargo-deny (?<version>\d+\.\d+\.\d+)$/u,
  },
});

const gateCommands = [
  { command: "gitleaks", args: ["version"] },
  { command: "cargo", args: ["audit", "--version"] },
  { command: "cargo", args: ["deny", "--version"] },
  {
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
  },
  { command: "pnpm", args: ["install", "--frozen-lockfile"] },
  { command: "pnpm", args: ["run", "check:toolchain"] },
  { command: "node", args: ["scripts/ci/verify-workflows.mjs"] },
  { command: "pnpm", args: ["run", "fmt:check"] },
  { command: "pnpm", args: ["run", "lint:ts"] },
  { command: "pnpm", args: ["run", "typecheck"] },
  { command: "pnpm", args: ["run", "test"] },
  { command: "pnpm", args: ["run", "build"] },
  { command: "cargo", args: ["test", "--workspace", "--locked"] },
  { command: "cargo", args: ["build", "--workspace", "--locked"] },
  {
    command: "cargo",
    args: [
      "clippy",
      "--workspace",
      "--all-targets",
      "--all-features",
      "--locked",
      "--",
      "-D",
      "warnings",
    ],
  },
  { command: "pnpm", args: ["run", "contracts:check"] },
  { command: "node", args: ["scripts/ci/check-rust-unsafe.mjs"] },
  { command: "pnpm", args: ["audit", "--audit-level=high"] },
  { command: "cargo", args: ["audit"] },
  { command: "cargo", args: ["deny", "check", "--config", ".github/cargo-deny.toml", "licenses"] },
  { command: "pnpm", args: ["run", "licenses:check"] },
];

// ADR-0005 fixes the canonical gate at 21 commands. This is a literal, not a
// derived value: it must not follow the list if the list is ever edited.
export const canonicalGateCommandCount = 21;

export function parseArguments(argv) {
  const options = {
    dryRun: false,
    evidencePath: undefined,
    platform: undefined,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--dry-run") {
      options.dryRun = true;
      continue;
    }
    if (argument === "--platform" || argument === "--evidence") {
      const value = argv[index + 1];
      if (!value || value.startsWith("--")) throw new Error(`${argument} requires a value`);
      if (argument === "--platform") options.platform = value;
      else options.evidencePath = value;
      index += 1;
      continue;
    }
    throw new Error(`unknown argument: ${argument}`);
  }

  if (options.platform !== "macos" && options.platform !== "ubuntu") {
    throw new Error("unsupported platform; use macos or ubuntu");
  }
  if (options.evidencePath && !isAbsolute(options.evidencePath)) {
    throw new Error(
      "--evidence must be an absolute path so SSH/rsync workflows never alter the checkout",
    );
  }

  return options;
}

export function createGatePlan() {
  if (gateCommands.length !== canonicalGateCommandCount) {
    throw new Error(
      `canonical gate must keep ${canonicalGateCommandCount} commands; found ${gateCommands.length}`,
    );
  }
  return {
    canonical_command_count: canonicalGateCommandCount,
    commands: gateCommands,
    companion_checks: companionCheckDefinitions(),
    runner: "local-environment",
  };
}

export function companionCheckDefinitions() {
  return [
    {
      command: "gitleaks",
      args: [
        "git",
        "--log-opts=--all",
        "--config",
        ".gitleaks.toml",
        "--redact",
        "--exit-code",
        "1",
        "--no-banner",
        ".",
      ],
      id: "full-refs-history-secret-scan",
      kind: "history-secret-scan",
    },
    { command: "pnpm", args: ["run", "verify:guards"], id: "tooling-guards", kind: "command" },
  ];
}

// Independent verification contract for companion evidence, not a demonstration
// that the current main gate is bypassable. The evidence records are passed in
// directly by the caller, and `main()` builds them in-process from the
// definitions above rather than reading an external evidence JSON. This binds
// each declared id to exactly one record per side, so a caller that later feeds
// in externally produced evidence cannot present a duplicate id or a swapped
// command as passing evidence.
export function assertMatchingCompanionEvidence(before, after) {
  const beforeRecords = before ?? [];
  const afterRecords = after ?? [];
  const unexpected = (records) =>
    records.filter((record) => !companionCheckIds.includes(record.id)).map((record) => record.id);
  for (const id of unexpected(beforeRecords).concat(unexpected(afterRecords))) {
    throw new Error(`companion evidence contains an undeclared check: ${id}`);
  }
  const definitions = new Map(
    companionCheckDefinitions().map((definition) => [definition.id, definition]),
  );
  for (const id of companionCheckIds) {
    const beforeMatches = beforeRecords.filter((record) => record.id === id);
    const afterMatches = afterRecords.filter((record) => record.id === id);
    if (beforeMatches.length > 1) {
      throw new Error(
        `companion check evidence has ${beforeMatches.length} records for a declared id before the canonical commands: ${id}`,
      );
    }
    if (afterMatches.length > 1) {
      throw new Error(
        `companion check evidence has ${afterMatches.length} records for a declared id after the canonical commands: ${id}`,
      );
    }
    const beforeRecord = beforeMatches[0];
    const afterRecord = afterMatches[0];
    if (!beforeRecord || !afterRecord) {
      throw new Error(`companion check evidence is missing: ${id}`);
    }
    for (const [side, record] of [
      ["before", beforeRecord],
      ["after", afterRecord],
    ]) {
      const definition = definitions.get(id);
      if (record.kind !== definition.kind) {
        throw new Error(
          `companion check kind does not match its definition on the ${side} side: ${id} (${record.kind})`,
        );
      }
      if (record.command !== definition.command) {
        throw new Error(
          `companion check command does not match its definition on the ${side} side: ${id} (${record.command})`,
        );
      }
      if (
        !Array.isArray(record.args) ||
        record.args.length !== definition.args.length ||
        record.args.some((argument, index) => argument !== definition.args[index])
      ) {
        throw new Error(
          `companion check args do not match its definition on the ${side} side: ${id}`,
        );
      }
    }
    if (beforeRecord.exit_code !== 0 || afterRecord.exit_code !== 0) {
      throw new Error(`companion check failed: ${id}`);
    }
    if ((beforeRecord.log_sha256 ?? null) !== (afterRecord.log_sha256 ?? null)) {
      throw new Error(`companion check log digest drifted: ${id}`);
    }
    if (
      (beforeRecord.history_scope?.refs_sha256 ?? null) !==
      (afterRecord.history_scope?.refs_sha256 ?? null)
    ) {
      throw new Error(`companion check history scope drifted: ${id}`);
    }
  }
}

function assertExpectedHost(options) {
  if (process.arch !== "arm64") {
    throw new Error("the selected platform gate must run on ARM64");
  }
  if (options.platform === "macos" && process.platform !== "darwin") {
    throw new Error("the macOS gate must run on macOS");
  }
  if (options.platform === "ubuntu" && process.platform !== "linux") {
    throw new Error("the Ubuntu gate must run directly on a Linux host");
  }
  if (options.platform === "ubuntu" && !isUbuntuOsRelease(readOsRelease())) {
    throw new Error("the Ubuntu gate requires an Ubuntu host");
  }
}

function runCommand({ command, args }, evidence) {
  process.stdout.write(`\n$ ${command} ${args.join(" ")}\n`);
  const startedAt = new Date().toISOString();
  const result = spawnSync(command, args, { cwd: root, env: process.env, stdio: "inherit" });
  const record = {
    args,
    command,
    ended_at: new Date().toISOString(),
    exit_code: result.status,
    started_at: startedAt,
  };
  if (result.error) record.start_error = result.error.message;
  evidence.commands.push(record);
  if (result.error) throw new Error(`could not start ${command}: ${result.error.message}`);
  if (result.status !== 0)
    throw new Error(`${command} exited with status ${result.status ?? "unknown"}`);
}

function renderPlan(plan) {
  for (const { command, args } of plan.commands) {
    process.stdout.write(`$ ${command} ${args.join(" ")}\n`);
  }
  process.stdout.write(
    `\nRequired companion evidence (recorded separately; canonical command count stays ${plan.canonical_command_count}):\n`,
  );
  for (const check of plan.companion_checks) {
    process.stdout.write(`[${check.id}] $ ${check.command} ${check.args.join(" ")}\n`);
  }
}

function capture(command, args) {
  const result = spawnSync(command, args, { cwd: root, encoding: "utf8" });
  const record = {
    command: [command, ...args].join(" "),
    exit_code: result.status,
    output: (result.stdout || result.stderr || "").trim(),
  };
  if (result.error) record.start_error = result.error.message;
  return record;
}

export function validateRequiredCapture(captureRecord, label, { allowEmpty = false } = {}) {
  if (!captureRecord) throw new Error(`${label} did not run`);
  if (captureRecord.start_error || captureRecord.exit_code !== 0) {
    throw new Error(`${label} failed`);
  }
  if (!allowEmpty && (!captureRecord.output || !captureRecord.output.trim())) {
    throw new Error(`${label} produced no parseable output`);
  }
  return captureRecord.output.trim();
}

export function validateToolchains(captures) {
  for (const [name, definition] of Object.entries(requiredToolchains)) {
    const output = validateRequiredCapture(captures[name], name);
    const match = definition.versionPattern.exec(output);
    if (!match?.groups?.version || match.groups.version !== definition.expectedVersion) {
      throw new Error(`${name} version mismatch; expected ${definition.expectedVersion}`);
    }
  }
}

function assertNormalizedRelativePath(inputPath) {
  if (typeof inputPath !== "string" || !inputPath || isAbsolute(inputPath)) {
    throw new Error("input path must be a normalized relative path");
  }
  const utf8Path = Buffer.from(inputPath, "utf8").toString("utf8");
  const segments = inputPath.split("/");
  const hasControlCharacter = [...inputPath].some((character) => {
    const codePoint = character.codePointAt(0);
    return codePoint <= 0x1f || codePoint === 0x7f;
  });
  if (
    utf8Path !== inputPath ||
    inputPath.normalize("NFC") !== inputPath ||
    hasControlCharacter ||
    inputPath.includes("\\") ||
    segments.some((segment) => !segment || segment === "." || segment === "..")
  ) {
    throw new Error(`input path is not a normalized relative path: ${inputPath}`);
  }
}

function isInsideDirectory(directory, candidate) {
  const pathFromDirectory = relative(directory, candidate);
  return (
    pathFromDirectory && !pathFromDirectory.startsWith(`..${sep}`) && pathFromDirectory !== ".."
  );
}

function relativeEvidencePath(repoRoot, evidencePath) {
  if (!evidencePath || !isInsideDirectory(repoRoot, evidencePath)) return undefined;
  return relative(repoRoot, evidencePath).split(sep).join("/");
}

function isExcludedInputPath(inputPath, evidencePath, tracked) {
  if (inputPath === evidencePath) return true;
  if (tracked) return false;
  const segments = inputPath.split("/");
  return segments.some(
    (segment) =>
      segment === ".git" ||
      segment === "node_modules" ||
      segment === "target" ||
      segment === ".pnpm-store" ||
      segment === "dist" ||
      segment.startsWith("._"),
  );
}

function decodeNullSeparatedUtf8(buffer) {
  const entries = [];
  let start = 0;
  for (let end = buffer.indexOf(0, start); end !== -1; end = buffer.indexOf(0, start)) {
    if (end !== start)
      entries.push(new TextDecoder("utf-8", { fatal: true }).decode(buffer.subarray(start, end)));
    start = end + 1;
  }
  if (start !== buffer.length) throw new Error("git input list was not null terminated");
  return entries;
}

// The ignored-file list of a real checkout exceeds Node's 1 MiB default pipe
// buffer (about 10 MiB here), and the gate must fail closed rather than report a
// truncated list that hides build inputs.
const gitListMaxBufferBytes = 64 * 1024 * 1024;

function captureGitFileList(repoRoot, args, label) {
  const result = spawnSync("git", args, {
    cwd: repoRoot,
    encoding: null,
    maxBuffer: gitListMaxBufferBytes,
  });
  if (result.error || result.status !== 0 || !Buffer.isBuffer(result.stdout)) {
    throw new Error(`${label} failed`);
  }
  try {
    return decodeNullSeparatedUtf8(result.stdout);
  } catch (error) {
    throw new Error(
      `${label} was not parseable: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}

// `git ls-files --others --exclude-standard` omits ignored paths by definition, so
// an ignored `.env*` file that a bundler would read is invisible to the input
// manifest. These helpers list ignored dotenv inputs by path only; no content is
// read, so no value can reach evidence or logs.
// `--directory` is deliberately not used: git then collapses untracked
// directories, which can hide an ignored dotenv file nested inside one (verified
// against the audit fixture). The full list is filtered here instead.
export function listIgnoredBuildInputPaths(repoRoot) {
  return captureGitFileList(
    repoRoot,
    ["ls-files", "-z", "--others", "--ignored", "--exclude-standard"],
    "git ignored list",
  ).filter((candidatePath) => {
    if (candidatePath.endsWith("/")) return false;
    const segments = candidatePath.split("/");
    if (segments.some((segment) => ignoredBuildInputDirectories.has(segment))) return false;
    const basename = segments.at(-1) ?? "";
    if (ignoredBuildInputNameExemptions.has(basename)) return false;
    return ignoredBuildInputBasenamePattern.test(basename);
  });
}

// Only a SHA-256 digest, byte size, and mode are recorded. Values are never
// decoded, logged, or written into evidence, so an env file cannot leak a secret
// through the gate, while two different external build inputs still produce two
// different gate identities.
export function buildIgnoredBuildInputManifest(inputFiles) {
  const entries = inputFiles
    .map(({ bytes, mode = "100644", path }) => {
      assertNormalizedRelativePath(path);
      if (!Buffer.isBuffer(bytes))
        throw new Error(`ignored build input ${path} was not read as bytes`);
      if (!/^100(?:644|755)$/u.test(mode)) {
        throw new Error(`ignored build input ${path} has unsupported mode`);
      }
      return {
        mode,
        path,
        sha256: createHash("sha256").update(bytes).digest("hex"),
        size_bytes: bytes.length,
      };
    })
    .sort((left, right) =>
      Buffer.compare(Buffer.from(left.path, "utf8"), Buffer.from(right.path, "utf8")),
    );
  for (let index = 1; index < entries.length; index += 1) {
    if (entries[index - 1].path === entries[index].path) {
      throw new Error(
        `ignored build input manifest contains duplicate path: ${entries[index].path}`,
      );
    }
  }
  const canonicalManifest = {
    encoding: "utf-8",
    entries,
    format: ignoredBuildInputFormat,
  };
  return {
    ...canonicalManifest,
    total_sha256: createHash("sha256")
      .update(JSON.stringify(canonicalManifest), "utf8")
      .digest("hex"),
  };
}

export function collectIgnoredBuildInputs({ dependencies = {}, repoRoot = root } = {}) {
  const resolvedRoot = resolve(repoRoot);
  const listIgnoredPaths = dependencies.listIgnoredPaths ?? listIgnoredBuildInputPaths;
  const lstat = dependencies.lstat ?? lstatSync;
  const readFile = dependencies.readFile ?? readFileSync;
  let ignoredPaths;
  try {
    ignoredPaths = listIgnoredPaths(resolvedRoot);
  } catch (error) {
    throw new Error(
      `could not list ignored build inputs: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
  if (!Array.isArray(ignoredPaths)) throw new Error("ignored build input list was not parseable");
  const inputFiles = [];
  for (const inputPath of ignoredPaths) {
    assertNormalizedRelativePath(inputPath);
    const absolutePath = resolve(resolvedRoot, inputPath);
    if (!isInsideDirectory(resolvedRoot, absolutePath)) {
      throw new Error(`ignored build input escaped repository: ${inputPath}`);
    }
    let stat;
    try {
      stat = lstat(absolutePath);
    } catch (error) {
      throw new Error(
        `could not inspect ignored build input ${inputPath}: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
    if (stat.isSymbolicLink())
      throw new Error(`ignored build input ${inputPath} is a symbolic link`);
    if (!stat.isFile()) throw new Error(`ignored build input ${inputPath} is a non-regular file`);
    let bytes;
    try {
      bytes = readFile(absolutePath);
    } catch (error) {
      throw new Error(
        `could not read ignored build input ${inputPath}: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
    if (!Buffer.isBuffer(bytes)) {
      throw new Error(`ignored build input ${inputPath} was not read as bytes`);
    }
    inputFiles.push({ bytes, mode: stat.mode & 0o111 ? "100755" : "100644", path: inputPath });
  }
  return buildIgnoredBuildInputManifest(inputFiles);
}

export function collectGateInputs({ dependencies = {}, evidencePath, repoRoot = root } = {}) {
  const resolvedRoot = resolve(repoRoot);
  const inputManifest = collectInputManifest({
    dependencies,
    evidencePath,
    repoRoot: resolvedRoot,
  });
  const ignoredBuildInputs = collectIgnoredBuildInputs({ dependencies, repoRoot: resolvedRoot });
  const identityBody = {
    ignored_build_inputs_sha256: ignoredBuildInputs.total_sha256,
    input_manifest_sha256: inputManifest.total_sha256,
  };
  return {
    ignored_build_inputs: ignoredBuildInputs,
    input_manifest: inputManifest,
    total_sha256: createHash("sha256")
      .update(JSON.stringify({ format: gateInputIdentityFormat, ...identityBody }), "utf8")
      .digest("hex"),
  };
}

export function assertMatchingGateInputs(before, after) {
  if (before?.total_sha256 !== after?.total_sha256) {
    throw new Error("gate inputs changed while gates were running");
  }
}

function listTrackedAndUntrackedPaths(repoRoot) {
  const deletedTrackedPaths = new Set(
    captureGitFileList(repoRoot, ["ls-files", "-z", "--deleted"], "git deleted tracked list"),
  );
  const tracked = captureGitFileList(
    repoRoot,
    ["ls-files", "-z", "--stage"],
    "git tracked list",
  ).map((record) => {
    const match = /^(?<mode>\d{6}) [0-9a-f]+ \d+\t(?<path>[\s\S]+)$/u.exec(record);
    if (!match?.groups) throw new Error("git tracked list contained an invalid stage record");
    return {
      deleted: deletedTrackedPaths.has(match.groups.path),
      mode: match.groups.mode,
      path: match.groups.path,
      tracked: true,
    };
  });
  const untracked = captureGitFileList(
    repoRoot,
    ["ls-files", "-z", "--others", "--exclude-standard"],
    "git untracked list",
  ).map((path) => ({ path, tracked: false }));
  return [...tracked, ...untracked];
}

export function buildInputManifest(inputFiles, { skippedTrackedDeletedPaths = [] } = {}) {
  const entries = inputFiles
    .map(({ bytes, mode = "100644", path }) => {
      assertNormalizedRelativePath(path);
      if (!Buffer.isBuffer(bytes)) throw new Error(`input ${path} was not read as bytes`);
      if (!/^100(?:644|755)$/u.test(mode)) throw new Error(`input ${path} has unsupported mode`);
      return {
        mode,
        path,
        sha256: createHash("sha256").update(bytes).digest("hex"),
        size_bytes: bytes.length,
      };
    })
    .sort((left, right) =>
      Buffer.compare(Buffer.from(left.path, "utf8"), Buffer.from(right.path, "utf8")),
    );
  for (let index = 1; index < entries.length; index += 1) {
    if (entries[index - 1].path === entries[index].path) {
      throw new Error(`input manifest contains duplicate path: ${entries[index].path}`);
    }
  }
  const sortedSkippedTrackedDeletedPaths = [...skippedTrackedDeletedPaths]
    .map((inputPath) => {
      assertNormalizedRelativePath(inputPath);
      return inputPath;
    })
    .sort((left, right) => Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8")));
  for (let index = 1; index < sortedSkippedTrackedDeletedPaths.length; index += 1) {
    if (sortedSkippedTrackedDeletedPaths[index - 1] === sortedSkippedTrackedDeletedPaths[index]) {
      throw new Error(
        `input manifest contains duplicate tracked deleted path: ${sortedSkippedTrackedDeletedPaths[index]}`,
      );
    }
  }
  const canonicalManifest = {
    encoding: "utf-8",
    entries,
    format: inputManifestFormat,
    skipped_tracked_deleted_paths: sortedSkippedTrackedDeletedPaths,
  };
  return {
    ...canonicalManifest,
    total_sha256: createHash("sha256")
      .update(JSON.stringify(canonicalManifest), "utf8")
      .digest("hex"),
  };
}

export function assertMatchingInputManifests(before, after) {
  if (!before?.total_sha256 || before.total_sha256 !== after?.total_sha256) {
    throw new Error("input manifest changed while gates were running");
  }
}

export function collectInputManifest({ dependencies = {}, evidencePath, repoRoot = root } = {}) {
  const resolvedRoot = resolve(repoRoot);
  const listPaths = dependencies.listPaths ?? listTrackedAndUntrackedPaths;
  const lstat = dependencies.lstat ?? lstatSync;
  const readFile = dependencies.readFile ?? readFileSync;
  let inputPaths;
  try {
    inputPaths = listPaths(resolvedRoot);
  } catch (error) {
    throw new Error(
      `could not list input files: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
  if (!Array.isArray(inputPaths)) throw new Error("input file list was not parseable");
  const evidenceRelativePath = relativeEvidencePath(resolvedRoot, evidencePath);
  const inputFiles = [];
  const skippedTrackedDeletedPaths = [];
  for (const descriptor of inputPaths) {
    const {
      deleted = false,
      path: inputPath,
      tracked = false,
    } = typeof descriptor === "string" ? { path: descriptor, tracked: false } : descriptor;
    assertNormalizedRelativePath(inputPath);
    if (isExcludedInputPath(inputPath, evidenceRelativePath, tracked)) continue;
    if (tracked && deleted) {
      skippedTrackedDeletedPaths.push(inputPath);
      continue;
    }
    const absolutePath = resolve(resolvedRoot, inputPath);
    if (!isInsideDirectory(resolvedRoot, absolutePath)) {
      throw new Error(`input path escaped repository: ${inputPath}`);
    }
    let stat;
    try {
      stat = lstat(absolutePath);
    } catch (error) {
      throw new Error(
        `could not inspect input ${inputPath}: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
    if (stat.isSymbolicLink()) throw new Error(`input ${inputPath} is a symbolic link`);
    if (!stat.isFile()) throw new Error(`input ${inputPath} is a non-regular file`);
    let bytes;
    try {
      bytes = readFile(absolutePath);
    } catch (error) {
      throw new Error(
        `could not read input ${inputPath}: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
    if (!Buffer.isBuffer(bytes)) throw new Error(`input ${inputPath} was not read as bytes`);
    const mode = descriptor.mode ?? (stat.mode & 0o111 ? "100755" : "100644");
    inputFiles.push({ bytes, mode, path: inputPath });
  }
  return buildInputManifest(inputFiles, { skippedTrackedDeletedPaths });
}

function readOsRelease() {
  try {
    return readFileSync("/etc/os-release", "utf8")
      .split(/\r?\n/u)
      .filter((line) => /^(ID|NAME|PRETTY_NAME|VERSION_ID)=/u.test(line))
      .sort();
  } catch {
    return ["unavailable"];
  }
}

export function isUbuntuOsRelease(lines) {
  return lines.some((line) => /^ID=(?:"ubuntu"|ubuntu)$/u.test(line));
}

export function assertCompleteHistoryScope(scope) {
  if (!scope || scope.is_shallow !== "false") {
    throw new Error("full history secret scan requires a non-shallow repository");
  }
  if (!Array.isArray(scope.refs) || scope.refs.length === 0) {
    throw new Error("full history secret scan recorded no refs");
  }
  if (!/^[a-f0-9]{64}$/u.test(scope.refs_sha256 ?? "")) {
    throw new Error("full history secret scan did not record a refs digest");
  }
  return scope;
}

// Tool output carries wall-clock timings, byte counts, and ANSI colour, so two
// identical results do not produce identical bytes. Only the stable, decision-
// bearing text is hashed; otherwise a timing change would read as hash drift
// and a real finding change could hide behind reformatting.
export function normalizeCompanionLog(log) {
  return (
    String(log)
      // eslint-disable-next-line no-control-regex -- matches ANSI colour codes in tool output
      .replace(/\u001B\[[0-9;]*m/gu, "")
      // Gitleaks 8.30 logs local 12-hour clocks. Strip only logger prefixes;
      // clocks inside payloads and the level/verdict/scope still affect the digest.
      .replace(
        /^(?:0?[1-9]|1[0-2]):[0-5]\d(?:AM|PM)(?=\s+(?:TRC|DBG|INF|WRN|ERR|FTL|PNC)\b)/gmu,
        "<timestamp>",
      )
      .replace(/\b\d+(?:[.,]\d+)?(?:ms|s|m|h|KB|MB|GB|B|bytes)\b/gu, "<measurement>")
      .replace(/\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?Z?/gu, "<timestamp>")
      .replace(/\s+/gu, " ")
      .trim()
  );
}

export function companionLogDigest(log) {
  return createHash("sha256").update(normalizeCompanionLog(log), "utf8").digest("hex");
}

function collectGitHistoryScope() {
  const isShallow = capture("git", ["rev-parse", "--is-shallow-repository"]);
  validateRequiredCapture(isShallow, "git shallow state");
  const refs = capture("git", ["for-each-ref", "--format=%(objectname) %(refname)"]);
  validateRequiredCapture(refs, "git refs");
  return assertCompleteHistoryScope({
    is_shallow: isShallow.output.trim(),
    refs: refs.output.split("\n").filter(Boolean),
    refs_sha256: createHash("sha256").update(refs.output, "utf8").digest("hex"),
  });
}

function runCompanionCheck(check, evidence) {
  process.stdout.write(`\n[companion:${check.id}] $ ${check.command} ${check.args.join(" ")}\n`);
  const startedAt = new Date().toISOString();
  const result = spawnSync(check.command, check.args, {
    cwd: root,
    env: process.env,
    encoding: "utf8",
  });
  const log = `${result.stdout ?? ""}${result.stderr ?? ""}`;
  process.stdout.write(log);
  const record = {
    args: check.args,
    command: check.command,
    ended_at: new Date().toISOString(),
    exit_code: result.status,
    id: check.id,
    kind: check.kind,
    // Raw bytes stay traceable in the log; the pass decision compares the
    // normalized digest so a timing change cannot masquerade as drift.
    log_sha256: companionLogDigest(log),
    started_at: startedAt,
  };
  if (result.error) record.start_error = result.error.message;
  if (check.kind === "history-secret-scan") record.history_scope = collectGitHistoryScope();
  evidence.companion_checks.push(record);
  if (result.error) throw new Error(`could not start ${check.id}: ${result.error.message}`);
  if (result.status !== 0)
    throw new Error(`${check.id} exited with status ${result.status ?? "unknown"}`);
  return record;
}

function collectHostEvidence(options, plan) {
  const head = capture("git", ["rev-parse", "HEAD"]);
  const status = capture("git", ["status", "--short"]);
  validateRequiredCapture(head, "git HEAD");
  validateRequiredCapture(status, "git status", { allowEmpty: true });
  const toolchainCaptures = Object.fromEntries(
    Object.entries(requiredToolchains).map(([name, { args, command }]) => [
      name,
      capture(command, args),
    ]),
  );
  validateToolchains(toolchainCaptures);
  const gateInputs = collectGateInputs({ evidencePath: options.evidencePath, repoRoot: root });
  const host = [capture("uname", ["-a"]), capture("uname", ["-m"])];
  validateRequiredCapture(host[0], "uname -a");
  validateRequiredCapture(host[1], "uname -m");
  return {
    architecture: process.arch,
    canonical_gate_command_count: plan.canonical_command_count,
    commands: [],
    companion_checks: [],
    gate_input_identity: gateInputs.total_sha256,
    ignored_build_inputs: gateInputs.ignored_build_inputs,
    os_release: process.platform === "linux" ? readOsRelease() : undefined,
    platform: process.platform,
    requested_platform: options.platform,
    started_at: new Date().toISOString(),
    workspace: [head, status],
    input_manifest: gateInputs.input_manifest,
    toolchains: Object.entries(toolchainCaptures).map(([name, record]) => ({
      ...record,
      expected_version: requiredToolchains[name].expectedVersion,
      name,
    })),
    host,
  };
}

// Every post-run condition an overall pass depends on lives here so each one is
// unit-testable: the canonical command count, the input identity across the run,
// and the required companion evidence.
export function assertCompletedGateRun({
  companionAfter,
  companionBefore,
  executedCommandCount,
  finalGateInputs,
  initialGateIdentity,
  plan,
}) {
  if (executedCommandCount !== plan.canonical_command_count) {
    throw new Error(
      `canonical gate must execute ${plan.canonical_command_count} commands; executed ${executedCommandCount}`,
    );
  }
  if (!initialGateIdentity?.total_sha256) {
    throw new Error("gate input identity evidence is missing");
  }
  assertMatchingGateInputs(initialGateIdentity, finalGateInputs);
  assertMatchingCompanionEvidence(companionBefore, companionAfter);
  return {
    canonical_commands_executed: executedCommandCount,
    gate_input_identity_verified_after_commands: finalGateInputs.total_sha256,
    ignored_build_inputs_verified_after_commands: finalGateInputs.ignored_build_inputs.total_sha256,
  };
}

function saveEvidence(options, evidence, result) {
  evidence.completed_at = new Date().toISOString();
  evidence.result = result;
  if (!options.evidencePath) return;
  writeFileSync(options.evidencePath, `${JSON.stringify(evidence, null, 2)}\n`, {
    encoding: "utf8",
    flag: "wx",
    mode: 0o600,
  });
  process.stdout.write(`Evidence written to ${options.evidencePath}\n`);
}

function main() {
  const options = parseArguments(process.argv.slice(2));
  const plan = createGatePlan(options);
  if (options.dryRun) {
    process.stdout.write(
      `Local ${options.platform} gate plan (${plan.runner}; no commands executed):\n`,
    );
    renderPlan(plan);
    return;
  }

  assertExpectedHost(options);
  const evidence = collectHostEvidence(options, plan);
  evidence.canonical_gate_command_count = plan.canonical_command_count;
  const companionBefore = [];
  try {
    for (const check of plan.companion_checks)
      runCompanionCheck(check, { companion_checks: companionBefore });
    for (const command of plan.commands) runCommand(command, evidence);
    const companionAfter = [];
    for (const check of plan.companion_checks)
      runCompanionCheck(check, { companion_checks: companionAfter });
    const finalGateInputs = collectGateInputs({
      evidencePath: options.evidencePath,
      repoRoot: root,
    });
    assertMatchingInputManifests(evidence.input_manifest, finalGateInputs.input_manifest);
    Object.assign(
      evidence,
      assertCompletedGateRun({
        companionAfter,
        companionBefore,
        executedCommandCount: evidence.commands.length,
        finalGateInputs,
        initialGateIdentity: { total_sha256: evidence.gate_input_identity },
        plan,
      }),
    );
    evidence.companion_checks = companionAfter;
    evidence.input_manifest_verified_after_commands = finalGateInputs.input_manifest.total_sha256;
    saveEvidence(options, evidence, "pass");
    process.stdout.write(`\nLocal ${options.platform} gate passed.\n`);
  } catch (error) {
    evidence.failure = error instanceof Error ? error.message : String(error);
    saveEvidence(options, evidence, "fail");
    throw error;
  }
}

const executedPath = process.argv[1] ? resolve(process.argv[1]) : undefined;
if (executedPath === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(
      `Local gate failed: ${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 1;
  }
}
