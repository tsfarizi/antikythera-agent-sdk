/**
 * Resolve version-manager shims before spawning a long-lived test server. Killing a shim can
 * leave its interpreter alive and keep inherited pipes open.
 */
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const candidate = process.env.PYTHON || 'python';
const probe = spawnSync(candidate, ['-c', 'import sys; print(sys.executable)'], { encoding:'utf8', windowsHide:true });
const executable = (probe.stdout || '').trim().split(/\r?\n/).at(-1);
if (probe.status !== 0 || !executable || !path.isAbsolute(executable) || !fs.existsSync(executable)) {
  throw new Error(`Cannot resolve Python interpreter from ${candidate}: ${probe.stderr || probe.error || probe.stdout}`);
}
export const PYTHON_CMD = executable;
