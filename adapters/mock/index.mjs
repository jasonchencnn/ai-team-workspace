#!/usr/bin/env node

import * as readline from 'node:readline';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

function getCliArg(prefix) {
  for (let i = 0; i < process.argv.length; i++) {
    if (process.argv[i] === prefix && process.argv[i + 1]) {
      return process.argv[i + 1];
    }
    if (process.argv[i].startsWith(prefix + '=')) {
      return process.argv[i].slice((prefix + '=').length);
    }
  }
  return null;
}

function getFaultMode(params) {
  if (params?.configuration?.fault_mode) {
    return params.configuration.fault_mode;
  }
  if (params?.extensions?.fault_mode) {
    return params.extensions.fault_mode;
  }
  if (params?.fault_mode) {
    return params.fault_mode;
  }
  return getCliArg('--fault-mode') || process.env.MOCK_FAULT_MODE || null;
}

function getFaultStage() {
  return getCliArg('--fault-stage') || process.env.MOCK_FAULT_STAGE || null;
}

const initialFaultMode = getFaultMode();
if (initialFaultMode === 'unresponsive' && getFaultStage() !== 'run') {
  process.stderr.write('Mock adapter unresponsive fault injection at startup\n');
  process.exit(137);
}

const detachedLifecycleMode = process.env.MOCK_DETACHED_LIFECYCLE_MODE;
if (detachedLifecycleMode) {
  const pidFile = process.env.MOCK_DETACHED_PID_FILE;
  if (!pidFile) {
    throw new Error('MOCK_DETACHED_PID_FILE is required for detached lifecycle mode');
  }
  const detachedChildScript = fileURLToPath(
    new URL('./detached-lifecycle-child.mjs', import.meta.url),
  );
  const child = spawn(process.execPath, [detachedChildScript], {
    detached: true,
    env: process.env,
    stdio: ['pipe', 'ignore', 'ignore'],
  });
  child.unref();
  child.stdin.on('error', () => {});

  if (detachedLifecycleMode === 'crash') {
    // Leave a brief observable interval so the integration test can prove the detached child
    // existed before the direct Adapter crashed.
    setTimeout(() => process.exit(42), 150);
  }
}

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout,
  terminal: false,
});

// `PROTOCOL.md` §2: stdout carries JSON Lines only. The `stdout_noise` fault mode deliberately
// breaks that rule so the supervisor's rejection can be exercised. The noise is configured with
// `MOCK_STDOUT_NOISE` (or `--stdout-noise`) as `|`-separated raw lines; it is written to stdout
// because the whole point of the fault is to pollute the protocol stream. `stderr` stays the
// diagnostic channel and records which stage was polluted. `MOCK_STDOUT_NOISE_STAGE` restricts the
// injection to a single stage so later stages can be reached after a clean handshake.
function configuredNoiseLines() {
  const raw = getCliArg('--stdout-noise') ?? process.env.MOCK_STDOUT_NOISE ?? null;
  // An explicitly empty value still means "inject a blank line", so `undefined` — not length —
  // decides whether noise is configured. Otherwise the blank-line case would inject nothing.
  if (raw === null) return null;
  return raw.split('|');
}

function emitStdoutNoise(stage) {
  const lines = configuredNoiseLines();
  if (lines === null) return;
  const only = getCliArg('--stdout-noise-stage') ?? process.env.MOCK_STDOUT_NOISE_STAGE ?? null;
  if (only !== null && only !== stage) return;
  for (const line of lines) {
    process.stdout.write(line + '\n');
  }
  process.stderr.write(`mock injected non-protocol stdout noise before ${stage}\n`);
}

function nowIso() {
  return new Date().toISOString();
}

function sendLine(obj) {
  process.stdout.write(JSON.stringify(obj) + '\n');
}

rl.on('line', (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;

  let msg;
  try {
    msg = JSON.parse(trimmed);
  } catch {
    sendLine({
      jsonrpc: '2.0',
      id: null,
      error: {
        code: -32700,
        message: 'Parse error',
      },
    });
    return;
  }

  const { id, method, params } = msg;
  const faultMode = getFaultMode(params);
  const faultStage = getFaultStage();

  if (method === 'initialize') {
    if (
      (faultMode === 'timeout' && faultStage !== 'run') ||
      faultMode === 'timeout_handshake'
    ) {
      // Deliberately hang/timeout during handshake
      return;
    }

    emitStdoutNoise('initialize');
    const capabilities =
      faultMode === 'missing_capability'
        ? ['health.check']
        : [
            'health.check',
            'run.start',
            'run.cancel',
          ];
    sendLine({
      jsonrpc: '2.0',
      id,
      result: {
        kind: 'initialize',
        protocol_version:
          faultMode === 'incompatible_protocol' ? '1.0.0' : '0.1.0',
        adapter_version: '0.1.0',
        harness_version: '1.0.0',
        capabilities,
        auth_status: faultMode === 'auth_missing' ? 'missing' : 'ready',
        isolation_mode: 'process',
        recovery_mode: 'none',
      },
    });
    return;
  }

  if (method === 'health.check') {
    emitStdoutNoise('health.check');
    const unavailable = faultMode === 'health_unavailable';
    sendLine({
      jsonrpc: '2.0',
      id,
      result: {
        kind: 'health',
        status: unavailable ? 'unavailable' : 'healthy',
        checked_at: nowIso(),
        diagnostic_codes: [unavailable ? 'ADAPTER_OFFLINE' : 'ALL_SYSTEMS_GO'],
      },
    });
    return;
  }

  if (method === 'run.cancel') {
    if (faultMode === 'reject_cancel') {
      sendLine({
        jsonrpc: '2.0',
        id,
        result: {
          kind: 'run_cancel',
          accepted: false,
          confirmed: false,
          observed_at: nowIso(),
        },
      });
      return;
    }

    if (faultMode === 'reject_cancel_timeout') {
      // Deliberately hang on cancel to simulate timeout
      return;
    }

    emitStdoutNoise('run.cancel');
    sendLine({
      jsonrpc: '2.0',
      id,
      result: {
        kind: 'run_cancel',
        accepted: true,
        confirmed: true,
        observed_at: nowIso(),
      },
    });
    return;
  }

  if (method === 'run.start') {
    if (
      (faultMode === 'timeout' && faultStage === 'run') ||
      faultMode === 'timeout_run'
    ) {
      // Deliberately hang/timeout during run start
      return;
    }

    if (faultMode === 'unresponsive' && faultStage === 'run') {
      rl.close();
      process.stdin.destroy();
      process.exit(137);
      return;
    }

    const {
      team_id,
      task_id,
      run_id,
      role_id,
      binding_id,
      binding_version,
      fencing_token,
    } = params;

    // 1. Acknowledge run.start request
    emitStdoutNoise('run.start');
    sendLine({
      jsonrpc: '2.0',
      id,
      result: {
        kind: 'run_start',
        accepted: true,
        session_reference: `session-mock-${run_id}`,
        observed_at: nowIso(),
      },
    });

    if (faultMode === 'timeout_stream') {
      // Hang after start acknowledgment
      return;
    }

    if (faultMode === 'crash') {
      // Send sequence 1 heartbeat then crash unexpectedly
      sendLine({
        jsonrpc: '2.0',
        method: 'run.heartbeat',
        params: {
          team_id,
          task_id,
          run_id,
          role_id,
          binding_id,
          binding_version,
          fencing_token,
          sequence: 1,
          observed_at: nowIso(),
          phase: 'executing',
          progress_percent: 25.0,
        },
      });
      process.stderr.write('Mock adapter crash fault injection\n');
      process.exit(42);
      return;
    }

    if (faultMode === 'out_of_order') {
      // Deliberately send sequence 2 before sequence 1
      sendLine({
        jsonrpc: '2.0',
        method: 'run.output',
        params: {
          team_id,
          task_id,
          run_id,
          role_id,
          binding_id,
          binding_version,
          fencing_token,
          sequence: 2,
          observed_at: nowIso(),
          kind: 'stdout',
          content: 'Out of order output\n',
          truncated: false,
        },
      });
      sendLine({
        jsonrpc: '2.0',
        method: 'run.heartbeat',
        params: {
          team_id,
          task_id,
          run_id,
          role_id,
          binding_id,
          binding_version,
          fencing_token,
          sequence: 1,
          observed_at: nowIso(),
          phase: 'executing',
          progress_percent: 25.0,
        },
      });
      return;
    }

    // Notification sequence=1: run.heartbeat
    emitStdoutNoise('run notification');
    const heartbeat1 = {
      jsonrpc: '2.0',
      method: 'run.heartbeat',
      params: {
        team_id,
        task_id,
        run_id,
        role_id,
        binding_id,
        binding_version,
        fencing_token,
        sequence: 1,
        observed_at: nowIso(),
        phase: 'executing',
        progress_percent: 25.0,
      },
    };
    sendLine(heartbeat1);

    if (faultMode === 'duplicate') {
      // Duplicate notification sequence=1 with identical payload
      sendLine(heartbeat1);
    }

    if (faultMode === 'duplicate_conflict') {
      // Same sequence with a different payload must be rejected as a protocol violation.
      sendLine({
        ...heartbeat1,
        params: {
          ...heartbeat1.params,
          phase: 'blocked',
        },
      });
      return;
    }

    // Notification sequence=2: run.output (stdout)
    sendLine({
      jsonrpc: '2.0',
      method: 'run.output',
      params: {
        team_id,
        task_id,
        run_id,
        role_id,
        binding_id,
        binding_version,
        fencing_token,
        sequence: 2,
        observed_at: nowIso(),
        kind: 'stdout',
        content: 'Mock execution started for task\n',
        truncated: false,
      },
    });

    // Notification sequence=3: run.output (artifact_candidate)
    sendLine({
      jsonrpc: '2.0',
      method: 'run.output',
      params: {
        team_id,
        task_id,
        run_id,
        role_id,
        binding_id,
        binding_version,
        fencing_token,
        sequence: 3,
        observed_at: nowIso(),
        kind: 'artifact_candidate',
        artifact: {
          kind: 'artifact',
          uri: `artifact://${run_id}/result.txt`,
          version: '1.0',
          checksum: 'sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
          media_type: 'text/plain',
        },
      },
    });

    // Notification sequence=4: run.completed
    sendLine({
      jsonrpc: '2.0',
      method: 'run.completed',
      params: {
        team_id,
        task_id,
        run_id,
        role_id,
        binding_id,
        binding_version,
        fencing_token,
        sequence: 4,
        completed_at: nowIso(),
        result_references: [
          {
            kind: 'artifact',
            uri: `artifact://${run_id}/result.txt`,
            version: '1.0',
            checksum: 'sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
            media_type: 'text/plain',
          },
        ],
        summary: 'Mock execution completed successfully.',
      },
    });

    if (faultMode === 'late_callback') {
      // Deliberately send late notification after completed terminal state
      sendLine({
        jsonrpc: '2.0',
        method: 'run.output',
        params: {
          team_id,
          task_id,
          run_id,
          role_id,
          binding_id,
          binding_version,
          fencing_token,
          sequence: 5,
          observed_at: nowIso(),
          kind: 'stdout',
          content: 'late output after completed\n',
          truncated: false,
        },
      });
    }

    return;
  }

  sendLine({
    jsonrpc: '2.0',
    id,
    error: {
      code: -32601,
      message: `Method not found: ${method}`,
    },
  });
});

rl.on('close', () => {
  if (detachedLifecycleMode === 'ignore_eof') {
    // Keep the direct Adapter alive until the supervisor reaches its force-close deadline.
    setInterval(() => {}, 1_000);
    return;
  }
  process.exit(0);
});
