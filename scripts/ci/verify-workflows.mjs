import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const workflowPath = resolve(root, ".github/workflows/ci.yml");
const dependabotPath = resolve(root, ".github/dependabot.yml");
const cargoDenyPath = resolve(root, ".github/cargo-deny.toml");

const workflow = read(workflowPath);
const dependabot = read(dependabotPath);
const cargoDeny = read(cargoDenyPath);

assert(
  /^on:\n {2}push:\n {4}branches: \[main\]$/m.test(workflow),
  "push CI must be limited to main; pull requests validate feature branches",
);

const requiredJobs = [
  "toolchain",
  "workflow-validation",
  "format",
  "lint",
  "typecheck",
  "tests",
  "rust-locked",
  "contracts-drift",
  "secret-scan",
  "history-secret-scan",
  "tooling-guards",
  "dependency-security",
  "license-compliance",
  "required-checks",
];

for (const job of requiredJobs) {
  assert(new RegExp(`^ {2}${job}:`, "m").test(workflow), `missing required job '${job}'`);
}

assert(workflow.includes("macos-latest"), "missing macOS coverage");
assert(!workflow.includes("ubuntu-24.04"), "Ubuntu must not block the Mac-first workflow");

for (const action of [
  "actions/checkout@v7.0.1",
  "actions/setup-node@v7.0.0",
  "pnpm/action-setup@v6.1.0",
  "gitleaks/gitleaks-action@v3.0.0",
]) {
  assert(workflow.includes(action), `action is not pinned to ${action}`);
}

for (const command of [
  "pnpm install --frozen-lockfile",
  "pnpm run check:toolchain",
  "pnpm run fmt:check",
  "pnpm run lint:ts",
  "pnpm run typecheck",
  "pnpm run test:${{ matrix.layer }}",
  "cargo test --workspace --locked",
  "cargo build --workspace --locked",
  "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings",
  "pnpm run contracts:check",
  "pnpm exec prettier --check .github/workflows/ci.yml",
  "pnpm audit --audit-level=high",
  "cargo audit",
  "cargo deny check --config .github/cargo-deny.toml licenses",
  "pnpm run licenses:check",
  "pnpm run verify:guards",
  "node scripts/ci/verify-workflows.mjs",
  "node scripts/ci/check-rust-unsafe.mjs",
]) {
  assert(workflow.includes(command), `missing gate command '${command}'`);
}

// The full history scan must read every ref, not only the checked-out branch,
// and must prove that scope locally instead of trusting the hosted action.
assert(
  workflow.includes(
    "gitleaks git --log-opts=--all --config .gitleaks.toml --redact --exit-code 1 --no-banner .",
  ),
  "the history scan must cover all refs with the reviewed policy",
);

const historyJob = extractWorkflowJob(workflow, "history-secret-scan");
assert(historyJob !== undefined, "the workflow must define a history-secret-scan job");
assert(
  historyJob.includes("fetch-depth: 0"),
  "the history scan job must fetch the complete history (fetch-depth: 0)",
);

const guardStep = findFailClosedShallowGuard(historyJob);
assert(
  guardStep !== undefined,
  "the history scan job must assert that the clone is not shallow before scanning",
);
assert(
  guardStep.index < findHistoryScanStepIndex(historyJob),
  "the shallow-repository assertion must run before the history secret scan",
);
assertFailsClosedOnShallowClone(guardStep.script);

assert(
  /^permissions:\n[ ]{2}contents: read$/m.test(workflow),
  "workflow must use least-privilege contents: read",
);
assert(workflow.includes("cache: pnpm"), "pnpm cache is not configured");
assert(
  workflow.includes("cache-dependency-path: pnpm-lock.yaml"),
  "pnpm cache must key from the lockfile",
);
assert(!workflow.includes("secrets."), "workflow must not pass repository secrets to jobs");
assert(
  workflow.includes("GITHUB_TOKEN: ${{ github.token }}"),
  "Gitleaks must receive only the automatic least-privilege GitHub token",
);
assert(
  workflow.includes("GITLEAKS_CONFIG: .gitleaks.toml"),
  "Gitleaks must use the reviewed repository policy",
);
assert(workflow.includes("needs:"), "required-checks must depend on individual gates");
for (const check of ["test", "build", "clippy"]) {
  assert(
    workflow.includes(`- os: macos-latest\n            check: ${check}`),
    `rust matrix is missing ${check} on macos-latest`,
  );
}
assert(
  /package-ecosystem:\s*"github-actions"/.test(dependabot),
  "Dependabot must cover GitHub Actions",
);
assert(/package-ecosystem:\s*"npm"/.test(dependabot), "Dependabot must cover npm dependencies");
assert(/package-ecosystem:\s*"cargo"/.test(dependabot), "Dependabot must cover Cargo dependencies");
assert(cargoDeny.includes("[licenses]"), "cargo-deny license policy is missing");
assert(
  read(resolve(root, "package.json")).includes(
    '"licenses:check": "node scripts/ci/check-node-licenses.mjs"',
  ),
  "root manifest must expose the Node transitive-license gate",
);
assert(
  read(resolve(root, "package.json")).includes(
    '"unsafe:check": "node scripts/ci/check-rust-unsafe.mjs"',
  ),
  "root manifest must expose the Rust unsafe gate",
);

process.stdout.write("CI workflow static contract passed.\n");

function read(path) {
  try {
    return readFileSync(path, "utf8");
  } catch (error) {
    if (error && typeof error === "object" && "code" in error && error.code === "ENOENT") {
      throw new Error(`missing required CI configuration: ${path}`, { cause: error });
    }
    throw error;
  }
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

// The workflow is YAML, but pulling in a parser is not permitted here, so the
// job body is located by indentation. Every top-level job sits at two spaces
// and its steps at six, which is what this repository's workflow uses.
function extractWorkflowJob(source, jobName) {
  const lines = source.split("\n");
  const start = lines.findIndex((line) => new RegExp(`^ {2}${jobName}:\\s*$`, "u").test(line));
  if (start === -1) return undefined;
  let end = start + 1;
  while (end < lines.length && !/^ {2}\S/u.test(lines[end])) end += 1;
  return lines.slice(start + 1, end).join("\n");
}

// Returns every `run` step of a job body as an executable script. A block
// scalar (`run: |`) must be detected before the inline form, otherwise the
// literal `|` is mistaken for the command itself.
function readStepScripts(jobBody) {
  const steps = [];
  const lines = jobBody.split("\n");
  for (const [offset, line] of lines.entries()) {
    if (!/^ {6}- /u.test(line)) continue;
    const block = [line];
    for (let index = offset + 1; index < lines.length; index += 1) {
      if (/^ {6}- /u.test(lines[index])) break;
      block.push(lines[index]);
    }
    const blockHeader = /^ {6}- run: \||^ {8}run: \|/u.exec(
      block.find((entry) => /^ {6}- run: \||^ {8}run: \|/u.test(entry)) ?? "",
    );
    if (blockHeader) {
      const script = [];
      for (const entry of block.slice(block.indexOf(blockHeader[0]) + 1)) {
        if (entry.trim() !== "" && !/^ {8,}\S/u.test(entry)) break;
        script.push(entry.replace(/^ {8}/u, ""));
      }
      steps.push({ index: steps.length, script: script.join("\n") });
      continue;
    }
    const inlineRun = /^ {6}- run: (?<command>[^|>].*)$/u.exec(line);
    if (inlineRun?.groups) {
      steps.push({ index: steps.length, script: inlineRun.groups.command });
    }
  }
  return steps;
}

function findHistoryScanStepIndex(jobBody) {
  return readStepScripts(jobBody).findIndex((step) => /^gitleaks git\b/u.test(step.script.trim()));
}

// A step qualifies as the guard only when it both reads the shallow state and
// can fail the job, so a step that merely prints the state cannot satisfy this.
// Whether it decides correctly is settled by executing it, not by its tokens.
function findFailClosedShallowGuard(jobBody) {
  return readStepScripts(jobBody).find(
    (step) =>
      /git rev-parse --is-shallow-repository/u.test(step.script) &&
      /\bexit\b/u.test(step.script) &&
      /!=|=|\[\[/u.test(step.script),
  );
}

// Executing the guard is what separates a real fail-closed assertion from a
// line that merely mentions the command. The stubbed `git` decides the outcome
// without needing a repository, and nothing here touches the network.
function assertFailsClosedOnShallowClone(script) {
  if (/\$\{\{/u.test(script)) {
    throw new Error("the shallow-repository assertion must not depend on workflow expressions");
  }
  const scenarios = [
    {
      description: "git reports a shallow repository",
      exitCode: 0,
      expected: "reject",
      stdout: "true\n",
    },
    {
      description: "git reports a complete clone",
      exitCode: 0,
      expected: "accept",
      stdout: "false\n",
    },
    {
      description: "git cannot determine the state",
      exitCode: 128,
      expected: "reject",
      stdout: "",
    },
    {
      description: "git reports an unparsable value",
      exitCode: 0,
      expected: "reject",
      stdout: "\n",
    },
  ];
  for (const scenario of scenarios) {
    const verdict = runShallowGuard(script, scenario);
    const rejected = verdict !== 0;
    const wanted = scenario.expected === "reject";
    if (rejected !== wanted) {
      throw new Error(
        `the shallow-repository assertion must ${wanted ? "fail" : "pass"} when ${scenario.description}; it exited ${verdict}`,
      );
    }
  }
}

function runShallowGuard(script, scenario) {
  const sandbox = mkdtempSync(join(tmpdir(), "verify-workflows-shallow-"));
  try {
    const stub = join(sandbox, "git");
    writeFileSync(
      stub,
      `#!/bin/sh\ncat <<'STDOUT'\n${scenario.stdout}STDOUT\nexit ${scenario.exitCode}\n`,
      { mode: 0o755 },
    );
    mkdirSync(join(sandbox, "work"), { recursive: true });
    const guardPath = join(sandbox, "guard.sh");
    writeFileSync(guardPath, script);
    const result = spawnSync("bash", [guardPath], {
      cwd: join(sandbox, "work"),
      encoding: "utf8",
      env: { ...process.env, PATH: `${sandbox}:${process.env.PATH ?? ""}` },
    });
    if (result.error) throw new Error(`could not run the shallow-repository assertion`);
    return result.status ?? 1;
  } finally {
    rmSync(sandbox, { force: true, recursive: true });
  }
}
