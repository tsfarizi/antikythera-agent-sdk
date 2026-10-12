/**
 * Component-base.test.mjs — U41: ekstensi minimal JS client runtime — opsi `componentBase` dan
 * `runner` pada createAgentRuntime (keputusan D5). Kontrak acuan: -
 * documentation/DECISIONS_RUNTIME_BRIDGE.md (D5) - documentation/WIRE_PROTOCOL.md §2.6
 * (component manifest) - clients/npm/antikythera-sdk/runtime/runner-core.js (loadRunnerModule)
 * Semantik yang difalsifikasi: - prioritas: runner (injeksi) > componentBase (manifest/URL) >
 * bundled path - runner diinjeksi -> dipakai tanpa import (tidak ada fetch manifest) -
 * componentBase + manifest -> import `${componentBase}/${entry}` - componentBase +
 * manifest/import gagal -> error eksplisit (bukan fallback) - tanpa kedua opsi -> bundled path
 * (default tidak berubah, tanpa fetch) Mekanisme: runtime client NYATA; server lokal menyajikan
 * manifest component + endpoint wire (pola FakeWireServer dari runtime-bridge.test.mjs). Node
 * tidak bisa import() skema http:, jadi componentBase memakai URL file: dan konstruksi URL
 * diverifikasi lewat behavior/error (logika resolusi identik dengan browser).
 */

'use strict';

import http from 'node:http';
import path from 'node:path';
import fs from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { test, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';

import rt from '../../clients/npm/antikythera-sdk/runtime/index.js';

const { createAgentRuntime, WIRE } = rt;


const HERE = path.dirname(fileURLToPath(import.meta.url));
const COMPONENT_DIR = path.resolve(HERE, '..', '..', 'clients', 'npm', 'antikythera-sdk', 'component');
const BUNDLED_ENTRY = 'antikythera-sdk.js';

const STUB_FINAL = '{"action":"final","content":"component-base-final"}';
const MANIFEST = { base: '/antikythera/v1/component/', entry: BUNDLED_ENTRY, version: '1.8.5' };

/**
 * ComponentTestServer — server lokal yang menyajikan manifest component + endpoint wire yang
 * dibutuhkan runtime (pola FakeWireServer).
 */

class ComponentTestServer {
  /**
   * @param {object} [options]
   * @param {object|null} [options.manifest] - component manifest; null => 404
   * @param {(index: number, body: object) => object} [options.llmResponder]
   */
  constructor({ manifest = MANIFEST, llmResponder } = {}) {
    this.manifest = manifest;
    this.llmResponder = llmResponder ?? (() => ({ content: STUB_FINAL }));
    this.llmCalls = [];
    this.manifestCalls = 0;
    this.getToolsCalls = 0;
    this.sseClients = new Map();
    this.server = null;
    this.port = 0;
  }

  url() {
    return `http://127.0.0.1:${this.port}`;
  }

  start() {
    return new Promise((resolve) => {
      this.server = http.createServer((req, res) => this.#handle(req, res));
      this.server.listen(0, '127.0.0.1', () => {
        this.port = this.server.address().port;
        resolve(this);
      });
    });
  }

  close() {
    return new Promise((resolve) => {
      for (const { res } of this.sseClients.values()) {
        try { res.end(); } catch { }
      }
      this.sseClients.clear();
      this.server.close(() => resolve());
    });
  }

  #sendJson(res, status, body) {
    res.writeHead(status, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(body));
  }

  #handle(req, res) {
    const url = new URL(req.url, 'http://x');

    if (req.method === 'GET' && url.pathname === WIRE.COMPONENT_MANIFEST) {
      this.manifestCalls += 1;
      if (this.manifest === null) {
        this.#sendJson(res, 404, { error: 'manifest disabled' });
        return;
      }
      this.#sendJson(res, 200, this.manifest);
      return;
    }

    /**
     * GET /antikythera/v1/component/{path} — static bundle files (MIME §2.6).
     */
    if (req.method === 'GET' && url.pathname.startsWith('/antikythera/v1/component/')) {
      const rel = url.pathname.slice('/antikythera/v1/component/'.length);
      const file = path.resolve(COMPONENT_DIR, rel);
      if (file.startsWith(COMPONENT_DIR) && fs.existsSync(file) && fs.statSync(file).isFile()) {
        const mime = rel.endsWith('.wasm')
          ? 'application/wasm'
          : rel.endsWith('.js') ? 'text/javascript' : 'application/octet-stream';
        res.writeHead(200, { 'Content-Type': mime });
        res.end(fs.readFileSync(file));
        return;
      }
      this.#sendJson(res, 404, { error: 'not found' });
      return;
    }

    if (req.method === 'POST' && url.pathname === WIRE.LLM_CALL) {
      let body = '';
      req.on('data', (c) => (body += c));
      req.on('end', () => {
        let parsed;
        try { parsed = JSON.parse(body); } catch { parsed = { raw: body }; }
        this.llmCalls.push(parsed);
        this.#sendJson(res, 200, this.llmResponder(this.llmCalls.length - 1, parsed));
      });
      return;
    }

    /**
     * GET /antikythera/v1/tools — registri server kosong (cukup untuk union).
     */
    if (req.method === 'GET' && url.pathname === WIRE.TOOLS_LIST) {
      this.getToolsCalls += 1;
      this.#sendJson(res, 200, []);
      return;
    }

    if (req.method === 'GET' && url.pathname === WIRE.EVENTS) {
      const clientId = url.searchParams.get('client_id');
      if (!clientId) {
        this.#sendJson(res, 400, { error: 'client_id is required' });
        return;
      }
      res.writeHead(200, {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        Connection: 'keep-alive',
      });
      res.write(': keepalive\n\n');
      this.sseClients.set(clientId, { res });
      req.on('close', () => this.sseClients.delete(clientId));
      return;
    }

    this.#sendJson(res, 404, { error: 'not found' });
  }
}

/**
 * Fetch probe (menangkap URL nyata yang dikirim runtime).
 */

let fetchStack = [];

function installFetchProbe() {
  const previous = globalThis.fetch;
  const calls = [];
  const probe = async (input, init) => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    const method = (init && init.method) || 'GET';
    let body = null;
    if (init && init.body !== undefined) {
      try { body = JSON.parse(init.body); } catch { body = init.body; }
    }
    calls.push({ url, method, body });
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
 * Runtime global hygiene: hooks provider adalah state global — setiap test harus reset (konvensi
 * runtime-bridge.test.mjs).
 */

function clearRuntimeHooksProvider() {
  delete globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__;
}

beforeEach(() => {
  clearRuntimeHooksProvider();
});

afterEach(() => {
  clearRuntimeHooksProvider();
  restoreAllFetchProbes();
});

/**
 * Injected runner stub — namespace 16 fungsi (permukaan runner komponen). Perilaku disengaja
 * BERBEDA dari komponen asli (session id + content commit) agar tes bisa membuktikan bahwa
 * namespace INI yang dieksekusi, bukan WASM.
 */

function makeInjectedRunner() {
  const calls = {
    init: 0,
    prepareUserTurn: 0,
    commitLlmResponse: 0,
    commitLlmStream: 0,
    drainEvents: 0,
    registerTools: 0,
  };
  const runner = {
    init() {
      calls.init += 1;
      return 'injected-session';
    },
    prepareUserTurn(json) {
      calls.prepareUserTurn += 1;
      const input = JSON.parse(json);
      return JSON.stringify({
        messages_json: JSON.stringify([{ role: 'user', content: input.prompt ?? '' }]),
        force_json: input.force_json ?? false,
        metadata_json: input.metadata_json ?? null,
      });
    },
    commitLlmResponse() {
      calls.commitLlmResponse += 1;
      return '{"action":"final","content":"injected-final"}';
    },
    commitLlmStream() {
      calls.commitLlmStream += 1;
      return '{"action":"final","content":"injected-stream-final"}';
    },
    appendLlmChunk() { return true; },
    processLlmResponseForSession() { return '{}'; },
    processToolResultForSession() {},
    drainEvents() { calls.drainEvents += 1; return '[]'; },
    getState() { return '{}'; },
    resetSession() { return true; },
    sweepIdleSessions() { return 0; },
    registerTools() { calls.registerTools += 1; return 0; },
    getToolsPrompt() { return ''; },
    setContextPolicy() { return true; },
    getTelemetrySnapshot() { return '{}'; },
    getSloSnapshot() { return '{}'; },
  };
  return { runner, calls };
}

/**
 * U41#1 — runner diinjeksi via opsi `runner`.
 */

test('U41#1 runner diinjeksi via opsi runner -> dipakai tanpa import', async () => {
  const server = await new ComponentTestServer().start();
  const probe = installFetchProbe();
  const { runner, calls } = makeInjectedRunner();
  let runtime = null;
  try {
    runtime = await createAgentRuntime({
      serverUrl: server.url(),
      runner,
      maxSteps: 5,
    });
    await runtime.connect();
    const result = await runtime.runTurn('injected turn');

    /**
     * Namespace INJEKSI yang dieksekusi (bukan WASM): session id dari stub, content commit dari
     * stub (komponen asli akan memakai content stub LLM).
     */
    assert.equal(runtime.sessionId, 'injected-session');
    assert.equal(result.action, 'final');
    assert.equal(result.content, 'injected-final');

    /**
     * Jalur pipeline melewati runner injeksi.
     */
    assert.ok(calls.init >= 1, 'injected runner init must be called');
    assert.ok(calls.prepareUserTurn >= 1, 'injected runner prepareUserTurn must be called');
    assert.ok(calls.commitLlmResponse >= 1, 'injected runner commitLlmResponse must be called');

    /**
     * Tanpa import: tidak ada fetch manifest sama sekali.
     */
    const manifestCalls = probe.calls.filter((c) => c.url.includes(WIRE.COMPONENT_MANIFEST));
    assert.equal(manifestCalls.length, 0, 'runner injection must not fetch the manifest');
    assert.equal(server.manifestCalls, 0, 'server must not receive a manifest request');
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    await server.close();
  }
});

test('U41#1b prioritas: runner menang atas componentBase', async () => {
  const server = await new ComponentTestServer().start();
  const { runner } = makeInjectedRunner();
  let runtime = null;
  try {
    runtime = await createAgentRuntime({
      serverUrl: server.url(),
      runner,
      /**
       * ComponentBase tidak boleh disentuh ketika runner diinjeksi.
       */
      componentBase: 'http://127.0.0.1:1/unreachable',
      maxSteps: 5,
    });
    await runtime.connect();
    assert.equal(runtime.sessionId, 'injected-session');
    assert.equal(server.manifestCalls, 0, 'manifest must not be fetched when runner is injected');
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    await server.close();
  }
});

/**
 * U41#2 — componentBase + manifest.
 */

test('U41#2 componentBase + manifest -> load dari URL yang benar (entry dari manifest)', async () => {
  const server = await new ComponentTestServer().start();
  const probe = installFetchProbe();
  let runtime = null;
  try {
    /**
     * PathToFileURL menghasilkan URL tanpa trailing slash — konstruksi URL componentBase/entry
     * harus menormalkan pemisahnya.
     */
    const componentBase = pathToFileURL(COMPONENT_DIR).href;
    runtime = await createAgentRuntime({
      serverUrl: server.url(),
      componentBase,
      maxSteps: 5,
    });
    await runtime.connect();
    const result = await runtime.runTurn('remote bundle turn');

    /**
     * Manifest di-fetch dari serverUrl dengan path component (tepat sekali).
     */
    const manifestCalls = probe.calls.filter((c) => c.method === 'GET' && c.url.includes(WIRE.COMPONENT_MANIFEST));
    assert.equal(manifestCalls.length, 1, 'manifest must be fetched exactly once');
    assert.equal(new URL(manifestCalls[0].url).host, new URL(server.url()).host);
    assert.equal(new URL(manifestCalls[0].url).pathname, WIRE.COMPONENT_MANIFEST);
    assert.equal(server.manifestCalls, 1, 'server must observe exactly one manifest request');

    /**
     * Bundle yang di-import dari componentBase adalah komponen NYATA: jalur WASM berjalan dan
     * commit memakai content stub LLM.
     */
    assert.equal(result.action, 'final');
    assert.equal(result.content, 'component-base-final');
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    await server.close();
  }
});

test('U41#2b componentBase + manifest gagal -> error eksplisit (bukan fallback)', async () => {
  const server = await new ComponentTestServer({ manifest: null }).start();
  let runtime = null;
  try {
    await assert.rejects(
      () => createAgentRuntime({
        serverUrl: server.url(),
        componentBase: pathToFileURL(COMPONENT_DIR).href,
      }),
      (err) => {
        const message = String(err.message);
        assert.match(message, /component manifest/);
        assert.match(message, /HTTP 404/);
        return true;
      },
    );
    assert.equal(server.manifestCalls, 1, 'manifest request must reach the server');
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    await server.close();
  }
});

test('U41#2c componentBase + import gagal -> error eksplisit berisi URL hasil konstruksi', async () => {
  const server = await new ComponentTestServer().start();
  let runtime = null;
  try {
    const missingDir = pathToFileURL(path.join(HERE, 'does-not-exist')).href;
    await assert.rejects(
      () => createAgentRuntime({
        serverUrl: server.url(),
        componentBase: missingDir,
      }),
      (err) => {
        const message = String(err.message);
        assert.match(message, /import component bundle/);
        /**
         * Entry berasal dari manifest -> URL lengkap componentBase/entry.
         */
        assert.ok(
          message.includes(`${missingDir}/instantiate.js`),
          `error must contain the constructed URL, got: ${message}`,
        );
        return true;
      },
    );
    assert.equal(server.manifestCalls, 1, 'manifest must be fetched before the import');
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    await server.close();
  }
});

/**
 * U41#3 — tanpa kedua opsi: bundled path (default tidak berubah).
 */

test('U41#3 tanpa kedua opsi -> bundled path (default tidak berubah)', async () => {
  const server = await new ComponentTestServer().start();
  const probe = installFetchProbe();
  let runtime = null;
  try {
    runtime = await createAgentRuntime({
      serverUrl: server.url(),
      maxSteps: 5,
    });
    await runtime.connect();
    const result = await runtime.runTurn('default turn');

    /**
     * Komponen bundled NYATA yang dieksekusi (jalur WASM berjalan).
     */
    assert.equal(result.action, 'final');
    assert.equal(result.content, 'component-base-final');

    /**
     * Kompatibel mundur penuh: default tidak memicu fetch manifest apa pun.
     */
    const manifestCalls = probe.calls.filter((c) => c.url.includes(WIRE.COMPONENT_MANIFEST));
    assert.equal(manifestCalls.length, 0, 'default path must not fetch the manifest');
    assert.equal(server.manifestCalls, 0, 'server must not receive a manifest request');
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    await server.close();
  }
});
