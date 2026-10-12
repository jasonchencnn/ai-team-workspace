#!/usr/bin/env node

import * as fs from 'node:fs';

// This child deliberately has its own session (`detached: true` at the call site). It is outside
// the supervisor's process group and must not rely on group termination. Its stdin is a control
// pipe held only by the direct Mock Adapter; EOF means that Adapter exited, was force-killed, or
// crashed.
const pidFile = process.env.MOCK_DETACHED_PID_FILE;
if (!pidFile) {
  throw new Error('MOCK_DETACHED_PID_FILE is required for detached lifecycle child');
}

fs.writeFileSync(pidFile, `${process.pid}\n`, { encoding: 'utf8', flag: 'wx' });
process.stdin.resume();
process.stdin.once('end', () => process.exit(0));
process.stdin.once('error', () => process.exit(0));
// A test interruption must not leave a detached process behind indefinitely.
setTimeout(() => process.exit(0), 5_000).unref();
