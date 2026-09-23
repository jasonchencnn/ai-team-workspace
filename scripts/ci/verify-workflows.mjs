import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const workflowPath = resolve(root, ".github/workflows/ci.yml");
const dependabotPath = resolve(root, ".github/dependabot.yml");
const cargoDenyPath = resolve(root, ".github/cargo-deny.toml");

const workflow = read(workflowPath);
const dependabot = read(dependabotPath);
const cargoDeny = read(cargoDenyPath);

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
  "dependency-security",
  "license-compliance",
  "required-checks",
];

for (const job of requiredJobs) {
  assert(new RegExp(`^ {2}${job}:`, "m").test(workflow), `missing required job '${job}'`);
}

for (const runner of ["ubuntu-latest", "macos-latest"]) {
  assert(workflow.includes(runner), `missing ${runner} coverage`);
}

for (const action of [
  "actions/checkout@v4.2.2",
  "actions/setup-node@v4.4.0",
  "pnpm/action-setup@v4.1.0",
  "gitleaks/gitleaks-action@v2.3.9",
  "docker://rhysd/actionlint:1.7.7",
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
  "pnpm audit --audit-level=high",
  "cargo audit",
  "cargo deny check --config .github/cargo-deny.toml licenses",
  "pnpm run licenses:check",
  "node scripts/ci/verify-workflows.mjs",
  "node scripts/ci/check-rust-unsafe.mjs",
]) {
  assert(workflow.includes(command), `missing gate command '${command}'`);
}

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
for (const dependency of [
  "libwebkit2gtk-4.1-dev",
  "libayatana-appindicator3-dev",
  "librsvg2-dev",
  "libxdo-dev",
  "patchelf",
]) {
  assert(workflow.includes(dependency), `missing Ubuntu desktop build dependency '${dependency}'`);
}
assert(workflow.includes("needs:"), "required-checks must depend on individual gates");
for (const runner of ["ubuntu-latest", "macos-latest"]) {
  for (const check of ["test", "build", "clippy"]) {
    assert(
      workflow.includes(`- os: ${runner}\n            check: ${check}`),
      `rust matrix is missing ${check} on ${runner}`,
    );
  }
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
