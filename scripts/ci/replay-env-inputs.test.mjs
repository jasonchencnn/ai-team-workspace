import assert from "node:assert/strict";
import { readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { runEnvInputReplay, withIsolatedFixture } from "./replay-env-inputs.mjs";

const ciDirectory = resolve(fileURLToPath(new URL(".", import.meta.url)));
const immutableHistoricalFixture = resolve(
  ciDirectory,
  "../../docs/reviews/evidence/deep-audit-2026-10-08/replay_env_manifest.mjs",
);

test("replays the legacy blind spot and the current gate identity", () => {
  const report = runEnvInputReplay();

  // RED, legacy: `git ls-files --others --exclude-standard` cannot see a
  // gitignored dotenv file, so two different files collapse into one identity.
  assert.equal(report.legacy_input_manifest.blind, true);
  assert.equal(
    report.legacy_input_manifest.empty_sha256,
    report.legacy_input_manifest.first_sha256,
  );
  assert.equal(
    report.legacy_input_manifest.first_sha256,
    report.legacy_input_manifest.second_sha256,
  );

  // GREEN, current: the gate identity binds the ignored dotenv input, so both
  // the empty and the changed fixtures are distinguishable.
  assert.equal(report.gate_input_identity.distinguishes, true);
  assert.notEqual(report.gate_input_identity.empty_sha256, report.gate_input_identity.first_sha256);
  assert.notEqual(
    report.gate_input_identity.first_sha256,
    report.gate_input_identity.second_sha256,
  );
  assert.equal(report.gate_input_identity.first_ignored_entries, 1);
  assert.equal(report.verified, true);
});

test("the ignored dotenv payload never appears in the replay output", () => {
  const serialized = JSON.stringify(runEnvInputReplay());
  assert.doesNotMatch(serialized, /REPLAY_FIXTURE_MARKER/u);
  assert.doesNotMatch(serialized, /alpha/u);
  assert.doesNotMatch(serialized, /beta/u);
});

test("the fixture is removed from the system temp directory", () => {
  const before = readdirSync(tmpdir()).filter((entry) => entry.startsWith("mm-fc09-env-input-"));
  runEnvInputReplay();
  const after = readdirSync(tmpdir()).filter((entry) => entry.startsWith("mm-fc09-env-input-"));
  assert.deepEqual(after, before);
});

test("a failing replay surfaces its own error", () => {
  assert.throws(
    () =>
      withIsolatedFixture(() => {
        throw new Error("replay body failed");
      }),
    /replay body failed/u,
  );
});

test("a failing cleanup cannot mask the original replay error", () => {
  // The immutable historical fixture threw from inside `finally`, so a cleanup
  // assertion replaced the original failure. Injecting a failing remover proves
  // the original error is what survives here, and that it stays first.
  let captured;
  try {
    withIsolatedFixture(
      () => {
        throw new Error("original replay failure");
      },
      { remove: () => new Error("cleanup failure") },
    );
  } catch (error) {
    captured = error;
  }
  assert.ok(captured instanceof AggregateError);
  assert.match(captured.errors[0].message, /original replay failure/u);
  assert.match(captured.errors[1].message, /cleanup failure/u);
});

test("a failing cleanup is still reported when the replay succeeded", () => {
  assert.throws(
    () => withIsolatedFixture(() => "ok", { remove: () => new Error("cleanup failure") }),
    /cleanup failure/u,
  );
});

test("a real cleanup failure still removes the fixture and is not swallowed", () => {
  // Probes the genuine remover rather than the injected stub: a fixture path
  // whose parent no longer exists cannot be removed, and that error must reach
  // the caller instead of being discarded.
  const missingParent = join(tmpdir(), "mm-fc09-missing-parent", "nested");
  assert.throws(
    () =>
      withIsolatedFixture(() => "ok", { remove: () => rmSync(missingParent, { recursive: true }) }),
    (error) => error instanceof Error,
    "a removal failure must not be silently ignored",
  );
});

test("the immutable historical fixture still throws from finally", () => {
  // Read-only historical material. This assertion documents why the new script
  // exists; it never edits, imports or executes that archived byte stream.
  const source = readFileSync(immutableHistoricalFixture, "utf8");
  assert.match(source, /finally\s*\{[\s\S]*?throw new Error\('unexpected fixture commit/u);
});
