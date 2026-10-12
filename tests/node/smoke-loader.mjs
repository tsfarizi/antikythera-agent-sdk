/**
 * Smoke: verify the http-bundle-loader materializes the Python-served bundle.
 */
import { PYTHON_CMD } from './python-executable.mjs';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import os from 'node:os';
import fs from 'node:fs';
import net from 'node:net';
import { installHttpBundleLoader } from './helpers/http-bundle-loader.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, '..', '..');
const PYTHON_PKG_DIR = path.join(REPO_ROOT, 'clients', 'python');
const COMPONENT_DIR = path.join(REPO_ROOT, 'clients', 'npm', 'antikythera-sdk', 'component');

function freePort() {
  return new Promise((resolve, reject) => {
    const probe = net.createServer();
    probe.once('error', reject);
    probe.listen(0, '127.0.0.1', () => {
      const port = probe.address().port;
      probe.close(() => resolve(port));
    });
  });
}

async function spawnServer(port) {
  const args = [
    '-m', 'antikythera_agent.server',
    '--bind', `127.0.0.1:${port}`,
    '--provider-stub', '{"action":"final","content":"live-ok"}',
    '--component-dir', COMPONENT_DIR,
    '--client-id', 'smoke-c1',
  ];
  const proc = spawn(PYTHON_CMD, args, { cwd: PYTHON_PKG_DIR, stdio: ['ignore', 'pipe', 'pipe'] });
  let out = '';
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`server start timeout: ${out}`)), 20000);
    proc.stdout.on('data', (d) => {
      out += d.toString();
      if (out.includes('listening on')) {
        clearTimeout(timer);
        resolve({ proc, url: `http://127.0.0.1:${port}` });
      }
    });
    proc.stderr.on('data', (d) => { out += d.toString(); });
    proc.on('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`server exited early (code ${code}): ${out}`));
    });
  });
}

const port = await freePort();
const { proc, url } = await spawnServer(port);
const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'antikythera-smoke-'));
try {
  const { registry, preload, deregister } = installHttpBundleLoader({
    baseUrl: `${url}/antikythera/v1/component/`,
    tempDir,
  });
  const entryHref = `${url}/antikythera/v1/component/antikythera-sdk.js`;
  const mod = await import(await preload(entryHref));
  const runner = mod.runner;
  console.log('runner.init type:', typeof runner.init);
  console.log('runner keys:', Object.keys(runner).slice(0, 20).join(','));
  console.log('fetches:');
  for (const f of registry.fetches) console.log('  ', f.kind.padEnd(3), f.status, f.url.replace(url, 'HOST'), f.bytes, 'bytes');
  console.log('total fetches:', registry.fetches.length);
  const wasmFetches = registry.fetches.filter((f) => f.kind === 'wasm');
  console.log('wasm fetches:', wasmFetches.length);
  console.log('all on server host:', registry.fetches.every((f) => f.url.startsWith(url)));
  deregister();
} finally {
  proc.kill();
  fs.rmSync(tempDir, { recursive: true, force: true });
}
