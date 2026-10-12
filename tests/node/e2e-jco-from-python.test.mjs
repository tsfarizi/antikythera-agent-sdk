/**
 * E2e-jco-from-python.test.mjs — U62: rantai E2E PENUH. Server bridge Python
 * (clients/python/antikythera_agent/server, U32) menyajikan bundle jco NYATA
 * (clients/npm/antikythera-sdk/component, 36 file) melalui `GET
 * /antikythera/v1/component/manifest` + `GET /antikythera/v1/component/{path}`; client JS
 * runtime (clients/npm/antikythera-sdk/runtime, U41) mengunduh manifest, memuat bundle dari
 * server, dan menjalankan turn sampai final dengan LLM stub. Kontrak acuan (klausa yang
 * difalsifikasi): - documentation/WIRE_PROTOCOL.md  §2.6 (manifest + MIME component) -
 * contracts/shared/wire_protocol.golden.json  (entry `component_manifest`) -
 * clients/python/antikythera_agent/server/__main__.py (CLI `--component-dir`, `--provider-stub`,
 * baris `[server-runtime] HTTP wire bridge listening on <url>`) -
 * clients/python/antikythera_agent/server/component.py (BASE_PATH, MIME_TYPES: .js =
 * text/javascript, .wasm = application/wasm) -
 * clients/python/antikythera_agent/server/transport.py (routes component) -
 * clients/npm/antikythera-sdk/runtime/runner-core.js (loadRunnerModule: componentBase + manifest
 * -> import `${componentBase}/${entry}`) - clients/npm/antikythera-sdk/runtime/types.js
 * (WIRE.COMPONENT_MANIFEST) Peta falsifikasi U62 (kontrak 1..6 dari plan eksekusi): 1. Spawn
 * server Python dengan --component-dir + --provider-stub   -> U62#1 2. Client mengunduh manifest
 * -> {base, entry, version}            -> U62#1 3. createAgentRuntime({serverUrl,
 * componentBase}) -> connect() -> runTurn(...) -> action=='final', content=='e2e-final'
 * -> U62#2 4. Bukti bundle dimuat dari server Python (fetch probe merekam permintaan ke
 * /antikythera/v1/component/ + verifikasi langsung fetch(componentBase+entry) = ESM valid)
 * -> U62#2 5. Bukti MIME: .js = text/javascript; .wasm = application/wasm    -> U62#3 6.
 * Pembersihan: close runtime, kill subprocess                    -> U62#1..3 Amplop Node
 * (konstrain yang DIdokumentasikan U41): `import()` Node tidak dapat memuat skema http: — jadi
 * bukti "bundle dimuat dari server Python" dilakukan dua lapis yang saling melengkapi: (a) HTTP
 * nyata: fetch probe merekam SEMUA permintaan ke componentBase (URL mengandung
 * /antikythera/v1/component/); verifikasi langsung fetch(componentBase+entry) = 200 +
 * text/javascript + ESM valid, fetch(componentBase+*.wasm) = 200 + application/wasm + magic
 * \0asm. (b) Eksekusi bytes identik: seluruh 36 file dimaterialisasi dari server Python ke temp
 * dir (bytes yang disajikan server diverifikasi IDENTIK dengan bundle di disk), lalu runtime
 * import dari mirror file: itu. Di browser target (D5/WIRE_PROTOCOL §2.6), komponen yang sama
 * di-import langsung dari URL http; mekanisme import berbeda, bytes yang dieksekusi identik.
 * Gaya: pola component-base.test.mjs (U41) + runtime-bridge.test.mjs (U12b). Tidak menduplikasi
 * U41: U41 memakai server lokal fake (ComponentTestServer) dan manifest statis; U62 memakai
 * server Python NYATA (subproses) + bundle nyata + stub LLM via CLI, dan membuktikan rantai
 * manifest->bundle->turn.
 */

'use strict';

import net from 'node:net';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawn } from 'node:child_process';
import { test, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';

import rt from '../../clients/npm/antikythera-sdk/runtime/index.js';

const { createAgentRuntime, WIRE } = rt;


const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, '..', '..');
const PYTHON_SRC = path.join(REPO_ROOT, 'clients', 'python');
const COMPONENT_DIR = path.resolve(HERE, '..', '..', 'clients', 'npm', 'antikythera-sdk', 'component');

/**
 * Stub LLM yang menjadi sumber kebenaran konten turn (kontrak U62 step 1).
 */
const STUB_E2E = '{"action":"final","content":"e2e-final"}';

/**
 * Manifest yang WAJIB disajikan server (golden `component_manifest`).
 */
const MANIFEST_BASE = '/antikythera/v1/component/';
const MANIFEST_ENTRY = 'antikythera-sdk.js';
const MANIFEST_VERSION = '1.8.5';

import { PYTHON_CMD } from './python-executable.mjs';

const activeSpawns = new Set();
const activeRuntimes = new Set();

async function terminateProc(proc, { timeoutMs = 4000 } = {}) {
  if (!proc || proc.exitCode !== null || proc.signalCode !== null) return;
  try { proc.stdout?.destroy(); } catch {}
  try { proc.stderr?.destroy(); } catch {}
  try { proc.kill(); } catch {}
  const exited = await Promise.race([
    new Promise((resolve) => proc.once("exit", () => resolve(true))),
    new Promise((resolve) => setTimeout(() => resolve(false), timeoutMs)),
  ]);
  if (!exited) {
    try { proc.kill("SIGKILL"); } catch {}
    await Promise.race([
      new Promise((resolve) => proc.once("exit", () => resolve())),
      new Promise((resolve) => setTimeout(resolve, 2000)),
    ]);
  }
  try { proc.stdout?.destroy(); } catch {}
  try { proc.stderr?.destroy(); } catch {}
  try { proc.stdin?.destroy(); } catch {}
}

function trackRuntime(rt) { if (rt) activeRuntimes.add(rt); return rt; }
function untrackRuntime(rt) { if (rt) activeRuntimes.delete(rt); }

/**
 * Process / port helpers (pola runtime-bridge.test.mjs).
 */

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

function pythonEnv() {
  const parts = [PYTHON_SRC, process.env.PYTHONPATH].filter(Boolean);
  return { ...process.env, PYTHONPATH: parts.join(path.delimiter) };
}

/**
 * Spawn `python -m antikythera_agent.server` (U32 CLI) dengan
 * `--component-dir COMPONENT_DIR` + `--provider-stub` dan tunggu baris
 * listening. Menangkap baris listening persis untuk validasi U62#1.
 * @returns {Promise<{proc: import('node:child_process').ChildProcess, port: number, url: string, listeningLine: string}>}
 */
function spawnPythonServer({ stubResponse = STUB_E2E } = {}) {
  return new Promise((resolve, reject) => {
    freePort().then((port) => {
      const args = [
        '-m', 'antikythera_agent.server',
        '--bind', `127.0.0.1:${port}`,
        '--component-dir', COMPONENT_DIR,
        '--provider-stub', stubResponse,
      ];
      const proc = spawn(PYTHON_CMD, args, {
        cwd: REPO_ROOT,
        env: pythonEnv(),
        stdio: ['ignore', 'pipe', 'pipe'],
        windowsHide: true,
      });
      activeSpawns.add(proc);
      proc.once("exit", () => activeSpawns.delete(proc));
      let out = '';
      let settled = false;
      const fail = (err) => {
        if (!settled) {
          settled = true;
          clearTimeout(timer);
          reject(err);
        }
      };
      const timer = setTimeout(() => {
        fail(new Error(`python server start timeout (${PYTHON_CMD}): ${out}`));
      }, 20000);
      proc.stdout.on('data', (d) => {
        out += d.toString();
        const match = out.match(/listening on (http:\/\/\S+)/);
        if (match && !settled) {
          settled = true;
          clearTimeout(timer);
          const line = out.split(/\r?\n/).find((l) => l.includes('listening on'))?.trim() ?? '';
          resolve({ proc, port, url: match[1], listeningLine: line });
        }
      });
      proc.stderr.on('data', () => {});
      proc.on('exit', (code) => {
        fail(new Error(`python server exited early (code ${code}): ${out}`));
      });
      proc.on('error', (err) => {
        fail(new Error(`failed to spawn python server (${PYTHON_CMD}): ${err.message}`));
      });
    }, reject);
  });
}

/**
 * Fetch probe (pola component-base.test.mjs / runtime-bridge.test.mjs).
 */

let fetchStack = [];

function installFetchProbe() {
  const previous = globalThis.fetch;
  const calls = [];
  const probe = async (input, init) => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    const method = (init && init.method) || 'GET';
    calls.push({ url, method, body: init?.body ?? null });
    return previous(input, init);
  };
  globalThis.fetch = probe;
  const record = { calls, restore: () => { globalThis.fetch = previous; } };
  fetchStack.push(record);
  return record;
}

function restoreAllFetchProbes() {
  while (fetchStack.length) fetchStack.pop().restore();
}

/**
 * Runtime global hygiene (konvensi runtime-bridge.test.mjs).
 */

function clearRuntimeHooksProvider() {
  delete globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__;
}

beforeEach(() => {
  clearRuntimeHooksProvider();
});

afterEach(async () => {
  restoreAllFetchProbes();
  clearRuntimeHooksProvider();
  for (const rt of [...activeRuntimes]) {
    try { rt.close(); } catch {}
  }
  activeRuntimes.clear();
  const kills = [...activeSpawns].map((p) => terminateProc(p));
  await Promise.allSettled(kills);
  activeSpawns.clear();
});

/**
 * Materialisasi bundle dari server Python ke mirror file: (bukti eksekusi bytes identik — amplop
 * Node tidak bisa import() skema http:).
 */

/**
 * Unduh seluruh file bundle dari `httpBase` (URL yang mengandung
 * /antikythera/v1/component/) ke temp dir. Setiap respons WAJIB status 200
 * dan bytes-nya IDENTIK dengan file di COMPONENT_DIR (server menyajikan
 * bundle nyata). Mengembalikan path temp dir mirror.
 */
async function materializeBundleFromServer(httpBase) {
  const tempRoot = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'antikythera-e2e-'));
  const relFiles = [];
  const walk = async (dir, prefix) => {
    for (const entry of await fs.promises.readdir(dir, { withFileTypes: true })) {
      const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
      if (entry.isDirectory()) {
        await walk(path.join(dir, entry.name), rel);
      } else {
        relFiles.push({ rel, abs: path.join(dir, entry.name) });
      }
    }
  };
  await walk(COMPONENT_DIR, '');

  for (const { rel, abs } of relFiles) {
    const url = httpBase + rel.split('/').map(encodeURIComponent).join('/');
    const res = await fetch(url);
    assert.equal(res.status, 200, `bundle file must be served with 200: ${rel}`);
    const bytes = Buffer.from(await res.arrayBuffer());
    const local = await fs.promises.readFile(abs);
    assert.ok(
      bytes.equals(local),
      `served bundle bytes must be identical to the real bundle: ${rel} (${local.length} local vs ${bytes.length} served)`,
    );
    const outPath = path.join(tempRoot, ...rel.split('/'));
    await fs.promises.mkdir(path.dirname(outPath), { recursive: true });
    await fs.promises.writeFile(outPath, bytes);
  }
  return tempRoot;
}

/**
 * U62#1 — kontrak 1 & 2: spawn CLI Python + manifest golden.
 */

test('U62#1 spawn Python bridge (--component-dir + --provider-stub) -> listening line persis + manifest shape golden', async () => {
  const spawned = await spawnPythonServer({ stubResponse: STUB_E2E });
  try {
    /**
     * Kontrak step 1: baris listening persis (parity test_bridge.py U61).
     */
    assert.equal(
      spawned.listeningLine,
      `[server-runtime] HTTP wire bridge listening on ${spawned.url}`,
      'listening line must match the Rust mirror format',
    );
    assert.equal(new URL(spawned.url).host, `127.0.0.1:${spawned.port}`);

    /**
     * Kontrak step 2: manifest dari GET /antikythera/v1/component/manifest.
     */
    const res = await fetch(`${spawned.url}${WIRE.COMPONENT_MANIFEST}`);
    assert.equal(res.status, 200);
    assert.ok(res.headers.get('content-type')?.includes('application/json'));

    const manifest = await res.json();
    /**
     * Shape persis golden `component_manifest`: {base, entry, version}, nol ekstra.
     */
    assert.deepEqual(Object.keys(manifest).sort(), ['base', 'entry', 'version'], 'manifest must have exactly base/entry/version');
    assert.equal(manifest.base, MANIFEST_BASE);
    assert.equal(manifest.entry, MANIFEST_ENTRY);
    assert.equal(manifest.version, MANIFEST_VERSION);
  } finally {
    await terminateProc(spawned.proc);
    activeSpawns.delete(spawned.proc);
  }
});

/**
 * U62#2 — kontrak 3 & 4: rantai penuh manifest -> bundle -> turn -> final.
 */

test('U62#2 rantai penuh: manifest dari server Python -> bundle nyata -> connect -> runTurn -> final e2e-final', async () => {
  const spawned = await spawnPythonServer({ stubResponse: STUB_E2E });
  const probe = installFetchProbe();
  let runtime = null;
  let tempDir = null;
  try {
    /**
     * (2) client mengunduh manifest dari server -> {base, entry, version}.
     */
    const manifest = await (await fetch(`${spawned.url}${WIRE.COMPONENT_MANIFEST}`)).json();
    assert.equal(manifest.entry, MANIFEST_ENTRY);

    /**
     * ComponentBase = serverUrl + base -> URL mengandung /antikythera/v1/component/.
     */
    const httpComponentBase = spawned.url + manifest.base;
    assert.ok(
      httpComponentBase.includes('/antikythera/v1/component/'),
      `componentBase URL must contain the component route, got: ${httpComponentBase}`,
    );

    /**
     * (4a) bukti HTTP langsung: entry = ESM valid (status 200, text/javascript).
     */
    const entryRes = await fetch(httpComponentBase + manifest.entry);
    assert.equal(entryRes.status, 200);
    assert.equal(entryRes.headers.get('content-type'), 'text/javascript');
    const entryText = await entryRes.text();
    assert.ok(entryText.includes('"use components"'), 'served entry must be jco ESM');
    assert.ok(entryText.includes('export'), 'served entry must contain ESM exports');

    /**
     * (4b) bukti HTTP langsung: wasm tersaji (status 200, application/wasm).
     */
    const wasmRes = await fetch(httpComponentBase + 'antikythera-sdk.runner.core.wasm');
    assert.equal(wasmRes.status, 200);
    assert.equal(wasmRes.headers.get('content-type'), 'application/wasm');
    const wasmMagic = Buffer.from(await wasmRes.arrayBuffer()).subarray(0, 4).toString('hex');
    assert.equal(wasmMagic, '0061736d', 'wasm magic bytes must be \\0asm');

    /**
     * (4c) eksekusi bytes identik: materialisasi 36 file dari server -> mirror.
     */
    tempDir = await materializeBundleFromServer(httpComponentBase);

    /**
     * (3) runtime client NYATA; loadRunnerModule mengunduh manifest dari server Python lalu
     * import entry dari mirror file: (bytes yang sama dengan yang disajikan server).
     */
    runtime = await createAgentRuntime({
      serverUrl: spawned.url,
      componentBase: pathToFileURL(tempDir).href,
      maxSteps: 5,
    });
    trackRuntime(runtime);
    await runtime.connect();

    /**
     * Session id berasal dari init WASM nyata (bukan runner injeksi).
     */
    assert.match(runtime.sessionId, /^session-/, 'session id must come from the real component init');

    const result = await runtime.runTurn('hello from jco via python');
    assert.equal(result.action, 'final');
    assert.equal(result.content, 'e2e-final');
    assert.equal(result.iterations, 1);
    assert.ok(Array.isArray(result.events), 'drained runner events must be an array');

    /**
     * (4d) fetch probe merekam permintaan nyata ke componentBase di server Python.
     */
    const componentRequests = probe.calls.filter((c) => c.url.includes('/antikythera/v1/component/'));
    assert.ok(
      componentRequests.length >= 2,
      `expected manifest + bundle requests to /antikythera/v1/component/, got ${componentRequests.length}`,
    );
    for (const call of componentRequests) {
      assert.equal(
        new URL(call.url).host,
        new URL(spawned.url).host,
        `component request must target the Python server host: ${call.url}`,
      );
    }
    /**
     * Manifest di-fetch oleh test DAN oleh loadRunnerModule runtime.
     */
    const manifestRequests = componentRequests.filter((c) => c.url.endsWith(WIRE.COMPONENT_MANIFEST));
    assert.ok(
      manifestRequests.length >= 2,
      `manifest must be fetched by the test and by the runtime, got ${manifestRequests.length}`,
    );
    /**
     * Entry di-fetch over HTTP dari server Python (verifikasi langsung + materialisasi).
     */
    const entryRequests = componentRequests.filter((c) => c.url.endsWith(`/${manifest.entry}`));
    assert.ok(
      entryRequests.length >= 1,
      `entry must be fetched over HTTP from the Python server, got ${entryRequests.length}`,
    );
  } finally {
    if (runtime) { try { runtime.close(); } catch {} untrackRuntime(runtime); }
    if (tempDir) await fs.promises.rm(tempDir, { recursive: true, force: true });
    await terminateProc(spawned.proc);
    activeSpawns.delete(spawned.proc);
  }
});

/**
 * U62#3 — kontrak 5: bukti MIME .js = text/javascript, .wasm = application/wasm.
 */

test('U62#3 bukti MIME: respons .js = text/javascript dan .wasm = application/wasm', async () => {
  const spawned = await spawnPythonServer({ stubResponse: STUB_E2E });
  try {
    const base = spawned.url + MANIFEST_BASE;

    const entryRes = await fetch(base + MANIFEST_ENTRY);
    assert.equal(entryRes.status, 200);
    assert.equal(entryRes.headers.get('content-type'), 'text/javascript');

    /**
     * Submodul .js (wasi-stubs) juga text/javascript.
     */
    const submoduleRes = await fetch(base + 'wasi-stubs/stdin.js');
    assert.equal(submoduleRes.status, 200);
    assert.equal(submoduleRes.headers.get('content-type'), 'text/javascript');

    /**
     * .wasm = application/wasm + magic bytes nyata.
     */
    const wasmRes = await fetch(base + 'antikythera-sdk.runner.core.wasm');
    assert.equal(wasmRes.status, 200);
    assert.equal(wasmRes.headers.get('content-type'), 'application/wasm');
    const wasmMagic = Buffer.from(await wasmRes.arrayBuffer()).subarray(0, 4).toString('hex');
    assert.equal(wasmMagic, '0061736d', 'wasm magic bytes must be \\0asm');
  } finally {
    await terminateProc(spawned.proc);
    activeSpawns.delete(spawned.proc);
  }
});
