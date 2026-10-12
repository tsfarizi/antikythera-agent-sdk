/**
 * Component-base-live.test.mjs — U42: client JS memuat runner jco via `componentBase` dari
 * server Python NYATA (bukan bundled path). Kontrak acuan (klausa yang difalsifikasi): -
 * documentation/DECISIONS_RUNTIME_BRIDGE.md (D4 manifest + bundle jco, D5 opsi
 * componentBase/runner, D2 drop-in peer Python) - documentation/WIRE_PROTOCOL.md §2.6 (component
 * manifest + MIME) - clients/python/antikythera_agent/server/__main__.py (CLI `--bind`,
 * `--provider-stub`, `--component-dir`, `--client-id`, baris `[server-runtime] HTTP wire bridge
 * listening on <url>`) - clients/python/antikythera_agent/server/component.py (BASE_PATH, ENTRY,
 * MIME_TYPES: .js = text/javascript) - clients/npm/antikythera-sdk/runtime/runner-core.js
 * (loadRunnerModule: componentBase -> manifest dari serverUrl -> import
 * `${componentBase}/${entry}`; komponen default TANPA componentBase TIDAK pernah fetch manifest)
 * Peta falsifikasi U42 (kontrak 1..4 dari plan eksekusi): 1. Spawn server Python NYATA
 * (--component-dir + --provider-stub + --client-id) -> GET /antikythera/v1/component/manifest ->
 * shape {base, entry, version} golden                           -> U42#1 2.
 * createAgentRuntime({serverUrl, componentBase}) -> connect() -> runTurn('hi') ->
 * action=='final', content=='live-ok'. Bukti "dimuat dari server Python": fetch probe merekam
 * manifest di-fetch dari host serverUrl + permintaan ke URL yang mengandung
 * /antikythera/v1/component/ + entry dieksekusi adalah bytes yang disajikan server
 * (materialisasi mirror).                    -> U42#2 3. Resolusi URL browser-equivalent:
 * componentBase = serverUrl + manifest.base -> runtime mengkonstruksi import persis
 * <serverUrl>/antikythera/v1/component/<entry> (ditolak Node karena skema http:, error memuat
 * URL lengkap)                      -> U42#2b 4. Verifikasi langsung fetch(componentBase +
 * entry) -> 200, content-type text/javascript, teks ESM valid (mengandung runner) -> U42#3 5.
 * Pembersihan: runtime.close() menutup SSE + menghapus hooks provider global; kill subprocess
 * menghentikan server; hooks global dibersihkan (delete
 * globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__)  -> U42#4 Amplop Node (konstrain platform
 * yang DIDOKUMENTASIKAN): `import()` Node tidak dapat memuat skema http:
 * (ERR_UNSUPPORTED_ESM_URL_SCHEME). Bukti "runner dimuat dari server Python" dibuat dua lapis
 * yang saling melengkapi (pola U62): (a) HTTP nyata: fetch probe merekam manifest yang di-fetch
 * runtime dari host serverUrl; seluruh 36 file bundle dimaterialisasi dari server dengan bytes
 * yang DIVERIFIKASI identik terhadap bundle nyata; entry juga diverifikasi langsung (U42#3);
 * U42#2b membuktikan URL import yang persis akan dipakai browser
 * (<serverUrl>/antikythera/v1/component/<entry>). (b) Eksekusi bytes identik: runtime import
 * entry dari mirror file: (bytes yang disajikan server). Di browser target (D5/WIRE_PROTOCOL
 * §2.6) komponen yang sama di-import langsung dari URL http; mekanisme Gaya: pola
 * e2e-jco-from-python.test.mjs (U62) + component-base.test.mjs (U41). Tidak menduplikasi U62:
 * U62 membuktikan rantai manifest->bundle->turn + MIME dengan componentBase mirror; U42
 * memfokuskan bukti loading via componentBase terhadap server PYTHON NYATA + resolusi URL
 * browser- equivalent + kontrak pembersihan. Catatan determinisme streaming: server Python
 * menyuntik `--client-id` yang TIDAK dicocokkan dengan clientId runtime (runtime memakai id
 * acak), sehingga push `llm-token` streaming jatuh ke client tak-terdaftar dan runtime mengambil
 * jalur stream-fallback yang BERTERMINASI (bounded 250ms, kontrak runner-core.js
 * commitStreamedTurn) — jalur yang sama yang dibuktikan U62. Semantik pengiriman token streaming
 * adalah kontrak unit lain (U61/U62 parity), bukan klausa U42; U42 membuktikan klausa loading
 * componentBase. Hasil turn identik pada kedua jalur: content stub 'live-ok' dari server.
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
 * Stub LLM yang menjadi sumber kebenaran konten turn (kontrak U42 step 2).
 */
const STUB_LIVE = '{"action":"final","content":"live-ok"}';

/**
 * Client id server yang dikontrak CLI `--client-id` (U42 step 1).
 */
const LIVE_CLIENT_ID = 'live-client-u42';

/**
 * Manifest yang WAJIB disajikan server (golden `component_manifest`).
 */
const MANIFEST_BASE = '/antikythera/v1/component/';
const MANIFEST_ENTRY = 'antikythera-sdk.js';
const MANIFEST_VERSION = '1.8.5';

import { PYTHON_CMD } from './python-executable.mjs';

/**
 * Process / port helpers (pola runtime-bridge.test.mjs / e2e U62).
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
 * Pelacak subproses aktif: safety net afterEach — test yang gagal sebelum finally tidak boleh
 * membocorkan server Python (menyebabkan suite menggantung).
 */
const activeSpawns = new Set();

/**
 * Spawn `python -m antikythera_agent.server` (U32 CLI) dengan
 * `--component-dir COMPONENT_DIR` + `--provider-stub` + `--client-id` dan
 * tunggu baris listening (timeout 15s). Mengembalikan proc/port/url/line.
 * @returns {Promise<{proc: import('node:child_process').ChildProcess, port: number, url: string, listeningLine: string}>}
 */
function spawnPythonServer({ stubResponse = STUB_LIVE, clientId = LIVE_CLIENT_ID } = {}) {
  return new Promise((resolve, reject) => {
    freePort().then((port) => {
      const args = [
        '-m', 'antikythera_agent.server',
        '--bind', `127.0.0.1:${port}`,
        '--provider-stub', stubResponse,
        '--component-dir', COMPONENT_DIR,
        '--client-id', clientId,
      ];
      const proc = spawn(PYTHON_CMD, args, {
        cwd: REPO_ROOT,
        env: pythonEnv(),
        stdio: ['ignore', 'pipe', 'pipe'],
        windowsHide: true,
      });
      activeSpawns.add(proc);
      proc.once('exit', () => activeSpawns.delete(proc));
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
      }, 15000);
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

/**
 * Pelacak runtime aktif: safety net afterEach — SSE channel yang bocor dari test gagal akan
 * menjaga event loop Node tetap hidup (hang suite).
 */
const activeRuntimes = new Set();

beforeEach(() => {
  clearRuntimeHooksProvider();
});

afterEach(() => {
  for (const proc of [...activeSpawns]) {
    try { proc.kill(); } catch { }
  }
  activeSpawns.clear();
  for (const rt of [...activeRuntimes]) {
    try { rt.close(); } catch { }
  }
  activeRuntimes.clear();
  clearRuntimeHooksProvider();
  restoreAllFetchProbes();
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
  const tempRoot = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'antikythera-live-'));
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
 * U42#1 — kontrak 1: spawn CLI Python + manifest golden.
 */

test('U42#1 spawn Python bridge (--component-dir + --provider-stub + --client-id) -> listening line persis + manifest shape golden', async () => {
  const spawned = await spawnPythonServer();
  try {
    /**
     * Kontrak step 1: baris listening persis (parity Rust mirror).
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
    spawned.proc.kill();
  }
});

/**
 * U42#2 — kontrak 2: componentBase memuat runner dari server Python NYATA.
 */

test('U42#2 componentBase memuat runner dari server Python NYATA: manifest dari serverUrl -> materialisasi bytes -> connect -> runTurn -> final live-ok (fetch probe bukti host serverUrl)', async () => {
  const spawned = await spawnPythonServer({ stubResponse: STUB_LIVE });
  const probe = installFetchProbe();
  let runtime = null;
  let tempDir = null;
  try {
    /**
     * (2a) client mengunduh manifest dari server -> {base, entry, version}.
     */
    const manifest = await (await fetch(`${spawned.url}${WIRE.COMPONENT_MANIFEST}`)).json();
    assert.equal(manifest.entry, MANIFEST_ENTRY);

    /**
     * ComponentBase = serverUrl + manifest.base -> URL mengandung route component.
     */
    const httpComponentBase = spawned.url + manifest.base;
    assert.ok(
      httpComponentBase.includes('/antikythera/v1/component/'),
      `componentBase URL must contain the component route, got: ${httpComponentBase}`,
    );
    assert.equal(new URL(httpComponentBase).host, new URL(spawned.url).host);

    /**
     * (2b) eksekusi bytes identik: materialisasi 36 file dari server -> mirror. Bytes yang
     * disajikan server diverifikasi IDENTIK dengan bundle nyata.
     */
    tempDir = await materializeBundleFromServer(httpComponentBase);

    /**
     * (2c) runtime client NYATA. loadRunnerModule mengunduh manifest DARI serverUrl (server
     * Python NYATA) lalu import entry dari mirror file: (bytes yang sama dengan yang disajikan
     * server).
     */
    runtime = await createAgentRuntime({
      serverUrl: spawned.url,
      componentBase: pathToFileURL(tempDir).href,
      maxSteps: 5,
    });
    activeRuntimes.add(runtime);
    await runtime.connect();

    /**
     * Session id berasal dari init WASM nyata (bukan runner injeksi).
     */
    assert.match(runtime.sessionId, /^session-/, 'session id must come from the real component init');

    const result = await runtime.runTurn('hi');
    assert.equal(result.action, 'final');
    assert.equal(result.content, 'live-ok');
    assert.equal(result.iterations, 1);
    assert.ok(Array.isArray(result.events), 'drained runner events must be an array');

    /**
     * (2d) fetch probe: SEMUA permintaan komponen menuju host serverUrl NYATA.
     */
    const componentRequests = probe.calls.filter((c) => c.url.includes('/antikythera/v1/component/'));
    /**
     * 36 file materialisasi + manifest (test + runtime) => >= 38.
     */
    assert.ok(
      componentRequests.length >= 36 + 2,
      `expected manifest + all bundle files fetched from the server, got ${componentRequests.length}`,
    );
    for (const call of componentRequests) {
      assert.equal(
        new URL(call.url).host,
        new URL(spawned.url).host,
        `component request must target the Python server host: ${call.url}`,
      );
    }

    /**
     * Manifest di-fetch oleh test DAN oleh loadRunnerModule runtime — ini bukti teras bahwa
     * componentBase TIDAK memakai bundled path (jalur default tanpa componentBase TIDAK pernah
     * fetch manifest, U41#3).
     */
    const manifestRequests = componentRequests.filter((c) => c.url.endsWith(WIRE.COMPONENT_MANIFEST));
    assert.ok(
      manifestRequests.length >= 2,
      `manifest must be fetched by the test and by the runtime from serverUrl, got ${manifestRequests.length}`,
    );
    for (const call of manifestRequests) {
      assert.equal(new URL(call.url).host, new URL(spawned.url).host);
    }

    /**
     * Entry di-fetch over HTTP dari server Python (materialisasi + verifikasi).
     */
    const entryRequests = componentRequests.filter((c) => c.url.endsWith(`/${manifest.entry}`));
    assert.ok(
      entryRequests.length >= 1,
      `entry must be fetched over HTTP from the Python server, got ${entryRequests.length}`,
    );
  } finally {
    if (runtime) {
      runtime.close();
      activeRuntimes.delete(runtime);
    }
    if (tempDir) await fs.promises.rm(tempDir, { recursive: true, force: true });
    spawned.proc.kill();
  }
});

/**
 * U42#2b — kontrak 3: resolusi URL browser-equivalent terhadap server NYATA.
 */

test('U42#2b resolusi URL browser-equivalent: componentBase = serverUrl + manifest.base -> import http: ditolak Node dengan URL server persis (manifest tetap di-fetch dari serverUrl)', async () => {
  const spawned = await spawnPythonServer({ stubResponse: STUB_LIVE });
  const probe = installFetchProbe();
  let runtime = null;
  try {
    const manifest = await (await fetch(`${spawned.url}${WIRE.COMPONENT_MANIFEST}`)).json();
    const httpComponentBase = spawned.url + manifest.base;

    /**
     * Jika Node bisa import() skema http:, runtime akan mengimpor PERSIS URL server ini. Node
     * menolak skema http: -> error eksplisit (bukan fallback senyap ke bundled path) yang memuat
     * URL konstruksi lengkap.
     */
    const expectedImportUrl = httpComponentBase + 'instantiate.js';
    await assert.rejects(
      () => createAgentRuntime({
        serverUrl: spawned.url,
        componentBase: httpComponentBase,
      }),
      (err) => {
        const message = String(err.message);
        assert.match(message, /import component bundle/);
        assert.ok(
          message.includes(expectedImportUrl),
          `error must contain the exact server import URL, got: ${message}`,
        );
        return true;
      },
    );

    /**
     * Sebelum mencoba import, loadRunnerModule HARUS sudah fetch manifest dari serverUrl (host
     * server Python NYATA) — inilah bukti sumber manifest.
     */
    const manifestRequests = probe.calls.filter((c) => c.url.includes(WIRE.COMPONENT_MANIFEST));
    assert.ok(
      manifestRequests.length >= 2,
      `manifest must be fetched by the test and by the runtime, got ${manifestRequests.length}`,
    );
    for (const call of manifestRequests) {
      assert.equal(
        new URL(call.url).host,
        new URL(spawned.url).host,
        `manifest must be fetched from the Python server host: ${call.url}`,
      );
    }
  } finally {
    if (runtime) {
      runtime.close();
      activeRuntimes.delete(runtime);
    }
    spawned.proc.kill();
  }
});

/**
 * U42#3 — kontrak 4: verifikasi langsung fetch(componentBase + entry).
 */

test('U42#3 verifikasi langsung fetch(componentBase + entry) -> 200 + content-type text/javascript + teks ESM valid (mengandung runner)', async () => {
  const spawned = await spawnPythonServer({ stubResponse: STUB_LIVE });
  try {
    const manifest = await (await fetch(`${spawned.url}${WIRE.COMPONENT_MANIFEST}`)).json();
    const httpComponentBase = spawned.url + manifest.base;

    const entryRes = await fetch(httpComponentBase + manifest.entry);
    assert.equal(entryRes.status, 200);
    assert.equal(entryRes.headers.get('content-type'), 'text/javascript');
    const entryText = await entryRes.text();
    /**
     * ESM valid: teks memuat runner namespace + deklarasi export.
     */
    assert.ok(entryText.includes('runner'), 'served entry must reference the runner namespace');
    assert.ok(entryText.includes('export'), 'served entry must contain ESM exports');
  } finally {
    spawned.proc.kill();
  }
});

/**
 * U42#4 — kontrak 5: pembersihan penuh.
 */

test('U42#4 pembersihan: runtime.close() menutup SSE + menghapus hooks provider global; kill subprocess menghentikan server; hooks global dibersihkan', async () => {
  const spawned = await spawnPythonServer({ stubResponse: STUB_LIVE });
  let runtime = null;
  let tempDir = null;
  try {
    const manifest = await (await fetch(`${spawned.url}${WIRE.COMPONENT_MANIFEST}`)).json();
    tempDir = await materializeBundleFromServer(spawned.url + manifest.base);
    runtime = await createAgentRuntime({
      serverUrl: spawned.url,
      componentBase: pathToFileURL(tempDir).href,
      maxSteps: 5,
    });
    activeRuntimes.add(runtime);
    await runtime.connect();
    const result = await runtime.runTurn('cleanup probe');
    assert.equal(result.content, 'live-ok');
    assert.equal(runtime.connected, true, 'runtime must be connected before cleanup');

    /**
     * (1) runtime.close(): SSE channel dihentikan, hooks provider global dihapus.
     */
    runtime.close();
    activeRuntimes.delete(runtime);
    assert.equal(runtime.connected, false, 'close() must disconnect the runtime');
    assert.equal(
      globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__,
      undefined,
      'close() must remove the global hooks provider',
    );

    /**
     * (2) kill subprocess: proses python harus benar-benar berhenti.
     */
    const exitObserved = new Promise((resolve) => {
      spawned.proc.once('exit', (code, signal) => resolve({ code, signal }));
    });
    spawned.proc.kill();
    assert.equal(spawned.proc.killed, true, 'kill() must be accepted by the subprocess');
    const exitInfo = await Promise.race([
      exitObserved,
      new Promise((resolve) => setTimeout(() => resolve(null), 8000)),
    ]);
    assert.ok(exitInfo, 'python server subprocess must exit within 8s of kill()');

    clearRuntimeHooksProvider();
    assert.equal(globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__, undefined);
  } finally {
    if (runtime) {
      runtime.close();
      activeRuntimes.delete(runtime);
    }
    if (tempDir) await fs.promises.rm(tempDir, { recursive: true, force: true });
    spawned.proc.kill();
  }
});
