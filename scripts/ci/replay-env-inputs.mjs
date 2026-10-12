#!/usr/bin/env node
// FC-09 executable replay.
//
// Two claims are replayed inside a throwaway Git repository created under the
// system temp directory:
//
//   1. RED, legacy claim: `collectInputManifest` (tracked + untracked,
//      `--exclude-standard`) is blind to a gitignored `.env.production`, so two
//      different files produce one identical `total_sha256`. This is the blind
//      spot the immutable audit fixture recorded.
//   2. GREEN, current claim: `collectGateInputs` binds ignored dotenv inputs
//      into the gate input identity, so the same two different files produce two
//      different identities.
//
// Nothing here writes a real `.env`, a real secret, or a real Git history: the
// fixture never commits, its dotenv payloads are synthetic markers, and the
// whole tree is removed afterwards.
//
// Cleanup structure is deliberate and is the point of this rewrite. The
// immutable historical fixture `docs/reviews/evidence/deep-audit-2026-10-08/
// replay_env_manifest.mjs` asserted an invariant by throwing from inside
// `finally`, which replaces the original failure with the assertion failure
// (`no-unsafe-finally`). Here cleanup never throws: `withIsolatedFixture`
// keeps the original error and, when cleanup also fails, reports both through
// an `AggregateError` with the original error first.
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { collectGateInputs, collectInputManifest } from "./local-gates.mjs";

const fixtureDirectoryPrefix = "mm-fc09-env-input-";

// Synthetic, non-secret marker payloads. They stand in for "different bytes"
// only; no configuration value from any real environment is used.
const firstIgnoredPayload = "REPLAY_FIXTURE_MARKER=alpha\n";
const secondIgnoredPayload = "REPLAY_FIXTURE_MARKER=beta\n";
const ignoredDotenvPath = ".env.production";

function asError(value) {
  return value instanceof Error ? value : new Error(String(value));
}

// Never throws. A removal failure is returned, not raised, so it can never
// replace an error raised by the replay body.
function removeFixture(fixturePath) {
  try {
    rmSync(fixturePath, { force: true, recursive: true });
    return null;
  } catch (error) {
    return asError(error);
  }
}

function initializeIsolatedRepository(fixturePath) {
  const init = spawnSync("git", ["init", "--quiet", "--template=", fixturePath], {
    encoding: "utf8",
  });
  if (init.status !== 0) {
    throw new Error(`isolated Git setup failed: ${init.stderr}`);
  }
}

// The historical fixture asserted "no commit exists" by throwing from inside
// `finally`. The same invariant is asserted here, in the body, where a failure
// is reported instead of masking anything.
function assertNoCommitWasCreated(fixturePath) {
  const head = spawnSync("git", ["-C", fixturePath, "rev-parse", "--verify", "HEAD"], {
    encoding: "utf8",
  });
  if (head.status === 0) {
    throw new Error("unexpected fixture commit; Git history was written");
  }
}

/**
 * Run `replay` against an isolated temporary repository and always remove it.
 *
 * `remove` is injectable so the test can force a cleanup failure and prove the
 * original error survives it.
 *
 * @param {((fixturePath: string) => unknown) | undefined} replay
 * @param {{ remove?: (fixturePath: string) => Error | null } | undefined} [options]
 * @returns {unknown}
 */
export function withIsolatedFixture(replay, { remove = removeFixture } = {}) {
  const fixturePath = mkdtempSync(join(tmpdir(), fixtureDirectoryPrefix));
  let result;
  let failure = null;
  try {
    initializeIsolatedRepository(fixturePath);
    assertNoCommitWasCreated(fixturePath);
    writeFileSync(resolve(fixturePath, ".gitignore"), ".env\n.env.*\n");
    writeFileSync(resolve(fixturePath, "package.json"), '{ "private": true }\n');
    result = replay(fixturePath);
  } catch (error) {
    failure = asError(error);
  }
  const cleanupFailure = remove(fixturePath);
  if (failure !== null && cleanupFailure !== null) {
    throw new AggregateError(
      [failure, cleanupFailure],
      "replay failed and fixture cleanup also failed; the original replay error is first",
    );
  }
  if (failure !== null) throw failure;
  if (cleanupFailure !== null) throw cleanupFailure;
  return result;
}

/**
 * Replay both claims inside one isolated repository.
 *
 * @param {string} fixturePath
 * @returns {{
 *   legacy_input_manifest: { blind: boolean, empty_sha256: string, first_sha256: string, second_sha256: string },
 *   gate_input_identity: { distinguishes: boolean, empty_sha256: string, first_sha256: string, second_sha256: string, first_ignored_entries: number },
 * }}
 */
export function replayEnvInputComparison(fixturePath) {
  const beforeLegacy = collectInputManifest({ repoRoot: fixturePath });
  const beforeGate = collectGateInputs({ repoRoot: fixturePath });

  writeFileSync(resolve(fixturePath, ignoredDotenvPath), firstIgnoredPayload);
  const firstLegacy = collectInputManifest({ repoRoot: fixturePath });
  const firstGate = collectGateInputs({ repoRoot: fixturePath });

  writeFileSync(resolve(fixturePath, ignoredDotenvPath), secondIgnoredPayload);
  const secondLegacy = collectInputManifest({ repoRoot: fixturePath });
  const secondGate = collectGateInputs({ repoRoot: fixturePath });

  return {
    gate_input_identity: {
      distinguishes:
        firstGate.total_sha256 !== secondGate.total_sha256 &&
        beforeGate.total_sha256 !== firstGate.total_sha256,
      empty_sha256: beforeGate.total_sha256,
      first_ignored_entries: firstGate.ignored_build_inputs.entries.length,
      first_sha256: firstGate.total_sha256,
      second_sha256: secondGate.total_sha256,
    },
    legacy_input_manifest: {
      blind:
        beforeLegacy.total_sha256 === firstLegacy.total_sha256 &&
        firstLegacy.total_sha256 === secondLegacy.total_sha256,
      empty_sha256: beforeLegacy.total_sha256,
      first_sha256: firstLegacy.total_sha256,
      second_sha256: secondLegacy.total_sha256,
    },
  };
}

/**
 * Run the replay in an isolated temporary repository and assert both claims.
 *
 * @returns {ReturnType<typeof replayEnvInputComparison> & { verified: boolean }}
 */
export function runEnvInputReplay() {
  const report = withIsolatedFixture(replayEnvInputComparison);
  const verified = report.legacy_input_manifest.blind && report.gate_input_identity.distinguishes;
  if (!verified) {
    throw new Error(
      `replay did not reproduce both claims: legacy_blind=${String(
        report.legacy_input_manifest.blind,
      )} gate_distinguishes=${String(report.gate_input_identity.distinguishes)}`,
    );
  }
  return { ...report, verified };
}

const invokedPath = process.argv[1];
const isDirectInvocation =
  invokedPath !== undefined && resolve(invokedPath) === resolve(fileURLToPath(import.meta.url));

if (isDirectInvocation) {
  // Only digests, entry counts and the boolean claims are printed. The ignored
  // dotenv payload never reaches stdout.
  process.stdout.write(`${JSON.stringify(runEnvInputReplay(), null, 2)}\n`);
}
