/**
 * Runtime-bridge.test.mjs — U12b, lane Node: client-side acceptance falsification for the
 * Antikythera runtime bridge (invarian 1–7 dari sisi CLIENT). Kontrak acuan: -
 * documentation/WIRE_PROTOCOL.md - contracts/shared/wire_protocol.golden.json -
 * clients/npm/antikythera-sdk/runtime/** (client runtime ASLI, jco composite) -
 * antikythera-server-runtime/src/main.rs (binary flags) Putaran KOREKSI (keputusan terkunci):
 * streaming disinyalkan via QUERY PARAM `?stream=true` pada POST /llm/call — BUKAN via
 * metadata_json; commit stream deterministik (settle, bukan race); close() membersihkan global
 * hooks provider; binary mendaftarkan tool server via `--server-tool <name>:<json>`. Mekanisme:
 * client runtime nyata (bukan simulasi). LLM selalu stub server. Dua jenis peer digunakan: 1.
 * binary Rust asli (target/debug/antikythera-server-runtime.exe) untuk semua jalur yang didukung
 * binary: proxy LLM, SSE lifecycle, streaming `?stream=true`, fail-closed /tools/execute, GET
 * /tools, `--server-tool` (registrasi tool server). 2. FakeWireServer (test-local, protokol
 * identik per golden) untuk skenario yang binary TIDAK bisa sediakan: stub LLM stateful
 * (call_tool → final), registri server berisi tool, dan dorongan envelope server→client.
 */

'use strict';

import http from 'node:http';
import net from 'node:net';
import path from 'node:path';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { test, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';

import rt from '../../clients/npm/antikythera-sdk/runtime/index.js';

const { createAgentRuntime, createTransport, createUnionRegistry, WIRE, buildToolCallEvent } = rt;


const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, '..', '..');
const SERVER_BIN = path.join(REPO_ROOT, 'target', 'debug', 'antikythera-server-runtime.exe');
const GOLDEN_PATH = path.join(REPO_ROOT, 'contracts', 'shared', 'wire_protocol.golden.json');

const STUB_FINAL = '{"action":"final","content":"stub-final"}';


let goldenCache = null;
function golden() {
  if (!goldenCache) {
    goldenCache = JSON.parse(fs.readFileSync(GOLDEN_PATH, 'utf8'));
  }
  return goldenCache;
}

/**
 * Wire-shape helpers (invarian 5: implementasi TIDAK boleh menambah field).
 */

function assertNoExtraKeys(actual, goldenSample, label) {
  assert.ok(actual && typeof actual === 'object', `${label}: actual is not an object`);
  const goldenKeys = Object.keys(goldenSample);
  const actualKeys = Object.keys(actual);
  const extra = actualKeys.filter((k) => !goldenKeys.includes(k));
  assert.deepEqual(
    extra,
    [],
    `${label}: wire object adds fields not present in golden: ${extra.join(', ')} (actual keys: ${actualKeys.join(', ')})`,
  );
}

function assertHasKeys(actual, requiredKeys, label) {
  const missing = requiredKeys.filter((k) => !(k in actual));
  assert.deepEqual(missing, [], `${label}: missing required wire fields: ${missing.join(', ')}`);
}

function assertWireObject(actual, goldenSample, label, requiredKeys) {
  assertNoExtraKeys(actual, goldenSample, label);
  if (requiredKeys) assertHasKeys(actual, requiredKeys, label);
  return actual;
}

function assertWireArrayOfObjects(actual, goldenSample, label) {
  assert.ok(Array.isArray(actual), `${label}: expected an array`);
  for (const item of actual) assertNoExtraKeys(item, goldenSample, label);
}


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

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitFor(fn, { timeoutMs = 5000, intervalMs = 20, label = 'condition' } = {}) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = await fn();
    if (value) return value;
    if (Date.now() > deadline) throw new Error(`timeout waiting for ${label}`);
    await sleep(intervalMs);
  }
}

/**
 * Spawn the real Rust binary and wait until it reports the listening line.
 * Returns { proc, port, url }.
 */
function spawnRealServer({ stubResponse = STUB_FINAL, clientId = null, allowTools = [], extraArgs = [] } = {}) {
  return new Promise((resolve, reject) => {
    freePort().then((port) => {
      const args = ['--bind', `127.0.0.1:${port}`, '--provider-stub', stubResponse];
      if (clientId) args.push('--client-id', clientId);
      for (const tool of allowTools) args.push('--allow-tool', tool);
      args.push(...extraArgs);
      const proc = spawn(SERVER_BIN, args, { stdio: ['ignore', 'pipe', 'pipe'] });
      let out = '';
      const timer = setTimeout(() => {
        reject(new Error(`server start timeout: ${out}`));
      }, 15000);
      proc.stdout.on('data', (d) => {
        out += d.toString();
        if (out.includes('listening on')) {
          clearTimeout(timer);
          resolve({ proc, port, url: `http://127.0.0.1:${port}` });
        }
      });
      proc.stderr.on('data', () => {});
      proc.on('exit', (code) => {
        clearTimeout(timer);
        reject(new Error(`server exited early (code ${code}): ${out}`));
      });
    }, reject);
  });
}

/**
 * FakeWireServer — test-local peer implementing the wire protocol exactly per the golden shapes.
 * Digunakan hanya untuk skenario yang binary asli tidak bisa sediakan (stub LLM stateful,
 * registri server berisi tool, envelope server→client). Client runtime tetap runtime ASLI.
 */

class FakeWireServer {
  /**
   * @param {object} options
   * @param {(index: number, body: object) => object} options.llmResponder -
   *   returns the llm-response wire object.
   * @param {Array<{definition: object, handler: (args: object) => any}>} [options.serverTools]
   * @param {Array<string>} [options.allowServerTools] - server-side allowlist for /tools/execute
   */
  constructor({ llmResponder, serverTools = [], allowServerTools = [] } = {}) {
    this.llmResponder = llmResponder ?? (() => ({ content: STUB_FINAL }));
    this.serverTools = new Map(serverTools.map((t) => [t.definition.name, t]));
    this.allowServerTools = new Set(allowServerTools);

    this.llmCalls = [];
    this.toolExecutes = [];
    this.getToolsCalls = 0;
    this.postbacks = []; /** {correlation_id, ok, payload, error} bodies. */
    this.llmResponses = []; /** Response objects actually sent on the wire. */
    this.sseClients = new Map(); /** Client_id -> { res, seq }. */
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

    if (req.method === 'POST' && url.pathname === WIRE.LLM_CALL) {
      let body = '';
      req.on('data', (c) => (body += c));
      req.on('end', () => {
        let parsed;
        try { parsed = JSON.parse(body); } catch { parsed = { raw: body }; }
        this.llmCalls.push(parsed);
        const response = this.llmResponder(this.llmCalls.length - 1, parsed);
        this.llmResponses.push(response);
        this.#sendJson(res, 200, response);
      });
      return;
    }

    if (req.method === 'POST' && url.pathname === WIRE.TOOLS_EXECUTE) {
      let body = '';
      req.on('data', (c) => (body += c));
      req.on('end', () => {
        let parsed;
        try { parsed = JSON.parse(body); } catch { parsed = { raw: body }; }
        this.toolExecutes.push(parsed);
        const entry = this.serverTools.get(parsed['tool-name']);
        if (!entry || !this.allowServerTools.has(parsed['tool-name'])) {
          this.#sendJson(res, 403, { error: `permission: tool '${parsed['tool-name'] ?? '?'}' not in allowlist` });
          return;
        }
        let args = {};
        try { args = JSON.parse(parsed['arguments-json'] ?? '{}'); } catch { args = {}; }
        const result = entry.handler(args);
        const response = {
          'tool-name': parsed['tool-name'],
          success: result.success !== false,
          'output-json': JSON.stringify(result.success === false ? { error: result.error } : result.output),
          'error-message': result.success === false ? (result.error ?? 'tool failed') : null,
          'step-id': typeof parsed['step-id'] === 'number' ? parsed['step-id'] : 0,
        };
        this.#sendJson(res, 200, response);
      });
      return;
    }

    if (req.method === 'GET' && url.pathname === WIRE.TOOLS_LIST) {
      this.getToolsCalls += 1;
      const defs = [...this.serverTools.values()].map((t) => t.definition);
      this.#sendJson(res, 200, defs);
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
      this.sseClients.set(clientId, { res, seq: 0 });
      this.pushEnvelope(clientId, {
        type: 'lifecycle',
        correlation_id: null,
        session_id: null,
        client_id: clientId,
        payload: { signal: 'connected' },
      });
      req.on('close', () => this.sseClients.delete(clientId));
      return;
    }

    const postbackMatch = url.pathname.match(/^\/antikythera\/v1\/events\/([^/]+)\/response$/);
    if (req.method === 'POST' && postbackMatch) {
      let body = '';
      req.on('data', (c) => (body += c));
      req.on('end', () => {
        let parsed;
        try { parsed = JSON.parse(body); } catch { parsed = { raw: body }; }
        this.postbacks.push(parsed);
        res.writeHead(204);
        res.end();
      });
      return;
    }

    this.#sendJson(res, 404, { error: 'not found' });
  }

  /** Push a wire envelope to an SSE client (server→client direction). */
  pushEnvelope(clientId, envelope) {
    const client = this.sseClients.get(clientId);
    if (!client) throw new Error(`pushEnvelope: no SSE client '${clientId}'`);
    client.seq += 1;
    client.res.write(`id: ${client.seq}\ndata: ${JSON.stringify(envelope)}\n\n`);
  }
}

/**
 * Fetch probe (menangkap URL + body nyata yang dikirim runtime).
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
 * Runtime global hygiene: hooks provider adalah state global (installRuntime- HooksProvider
 * tidak membersihkan dirinya sendiri) — setiap test harus reset.
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
 * Tool definition factory (shape per golden tools_list_response).
 */

function makeToolDef(name, description, extra = {}) {
  return {
    name,
    title: `${name} title`,
    description,
    parameters: [],
    input_schema: { type: 'object', properties: {}, required: [] },
    output_schema: null,
    ...extra,
  };
}

function makeToolEntry(name, description, handler, extra = {}) {
  return { definition: makeToolDef(name, description, extra), handler };
}


function assertPermissionError(err, label) {
  const message = err instanceof Error ? err.message : String(err);
  assert.ok(message.startsWith('permission:'), `${label}: expected permission: denial, got: ${message}`);
}

/**
 * T1 — core@client → tool client.
 */

test('U12b#1 core@client -> tool client: stub LLM call_tool get_time -> handler runs, result in session, final', async () => {
  let handlerCalls = 0;
  const server = await new FakeWireServer({
    llmResponder: (index) => ({
      content: index === 0
        ? '{"action":"call_tool","tool":"get_time","input":{}}'
        : '{"action":"final","content":"after-tool"}',
      model: 'stub',
      session_id: null,
      message_json: null,
      tokens_used: 1,
      finish_reason: 'stop',
      raw_response_json: null,
    }),
  }).start();

  const probe = installFetchProbe();
  let runtime = null;
  try {
    runtime = await createAgentRuntime({
      serverUrl: server.url(),
      tools: [makeToolEntry('get_time', 'Get the current time', async () => {
        handlerCalls += 1;
        return { now: '2026-08-13T00:00:00Z' };
      })],
      policy: { allow: ['get_time'] },
      maxSteps: 5,
    });
    await runtime.connect();
    const result = await runtime.runTurn('what time is it?');

    assert.equal(result.action, 'final');
    assert.equal(result.content, 'after-tool');
    assert.equal(result.iterations, 2);
    assert.equal(handlerCalls, 1, 'local handler must execute exactly once');

    /**
     * Hasil masuk session: runner me-record tool_result + final di drain.
     */
    const toolResultEvent = result.events.find((e) => e.kind === 'tool_result');
    assert.ok(toolResultEvent, 'drained events must contain tool_result');
    assert.equal(toolResultEvent.payload.tool, 'get_time');
    assert.equal(toolResultEvent.payload.success, true);

    /**
     * Tool dijalankan LOKAL, bukan lewat server.
     */
    assert.equal(server.toolExecutes.length, 0, 'client-owned tool must not hit POST /tools/execute');

    /**
     * Wire shape: llm-request yang benar-benar dikirim subset dari golden.
     */
    assert.ok(server.llmCalls.length === 2, `expected 2 llm calls, got ${server.llmCalls.length}`);
    const llmReq = server.llmCalls[0];
    assertWireObject(llmReq, golden().llm_call_request, 'llm_call_request (T1)', [
      'provider', 'model', 'session_id', 'messages_json', 'force_json', 'temperature', 'max_tokens', 'schema_name', 'metadata_json',
    ]);
    /**
     * Streaming disinyalkan via QUERY PARAM `?stream=true` (keputusan U12b), TIDAK lewat
     * metadata_json — metadata_json tetap metadata provider.
     */
    const llmPost = probe.calls.find((c) => c.method === 'POST' && c.url.includes(WIRE.LLM_CALL));
    assert.ok(llmPost, 'POST /llm/call must be observed');
    assert.ok(
      new URL(llmPost.url).searchParams.get('stream') === 'true',
      `streaming must be signaled by ?stream=true, got URL: ${llmPost.url}`,
    );
    assert.equal(JSON.parse(llmReq.metadata_json ?? '{}').stream, undefined, 'stream flag must not live in metadata_json');
    assert.equal(llmReq.metadata_json, null, 'metadata_json must stay untouched without provider metadata');
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    await server.close();
  }
});

/**
 * T2 — core@client → tool server.
 */

test('U12b#2 core@client -> tool server: stub LLM call_tool srv_echo -> POST /tools/execute diterima, hasil diproses, final', async () => {
  const server = await new FakeWireServer({
    serverTools: [makeToolEntry('srv_echo', 'Server-side echo', (args) => ({ echoed: args }))],
    allowServerTools: ['srv_echo'],
    llmResponder: (index) => ({
      content: index === 0
        ? '{"action":"call_tool","tool":"srv_echo","input":{"x":1}}'
        : '{"action":"final","content":"server-tool-done"}',
      model: 'stub',
      session_id: null,
      message_json: null,
      tokens_used: 1,
      finish_reason: 'stop',
      raw_response_json: null,
    }),
  }).start();

  const runtime = await createAgentRuntime({ serverUrl: server.url(), maxSteps: 5 });
  try {
    await runtime.connect();

    /**
     * Registry union: tool server terlihat sebagai owner 'server'.
     */
    const registry = await runtime.refreshTools();
    assert.equal(registry.ownerOf('srv_echo'), 'server');

    const result = await runtime.runTurn('call srv_echo');
    assert.equal(result.action, 'final');
    assert.equal(result.content, 'server-tool-done');

    /**
     * Server benar-benar menerima POST /tools/execute dengan shape golden.
     */
    assert.equal(server.toolExecutes.length, 1, 'server tool must be executed via POST /tools/execute');
    const exec = server.toolExecutes[0];
    assertWireObject(exec, golden().tool_execute_request, 'tool_execute_request (T2)', [
      'tool-name', 'arguments-json', 'session-id', 'step-id',
    ]);
    assert.equal(exec['tool-name'], 'srv_echo');
    assert.equal(exec['arguments-json'], JSON.stringify({ x: 1 }));

    /**
     * Hasil masuk session: event tool_result ada di drain.
     */
    const toolResultEvent = result.events.find((e) => e.kind === 'tool_result');
    assert.ok(toolResultEvent, 'drained events must contain tool_result');
    assert.equal(toolResultEvent.payload.tool, 'srv_echo');
  } finally {
    runtime.close();
    clearRuntimeHooksProvider();
    await server.close();
  }
});

test('U12b#2b binary Rust: /tools/execute fail-closed untuk tool server tak terdaftar (gap API, bukan happy path)', async () => {
  /**
   * Binary asli hanya mendaftarkan tool server via `--server-tool` (registrasi = grant).
   * `--allow-tool` hanya mengubah policy allowlist; tanpa registrasi, registry tetap kosong dan
   * POST /tools/execute fail-closed (lihat antikythera-server-runtime/src/main.rs).
   */
  const { proc, url } = await spawnRealServer({
    stubResponse: '{"action":"call_tool","tool":"srv_echo","input":{}}',
    allowTools: ['srv_echo'],
  });
  const probe = installFetchProbe();
  let runtime = null;
  try {
    const transport = createTransport({ serverUrl: url });

    /**
     * A) POST /tools/execute dengan shape golden → server tolak fail-closed.
     */
    await assert.rejects(
      () => transport.executeServerTool(buildToolCallEvent({
        toolName: 'srv_echo', argumentsJson: '{"x":1}', sessionId: 's-gap', stepId: 1,
      })),
      (err) => { assertPermissionError(err, 'binary tools/execute'); return true; },
    );
    const posted = probe.calls.filter((c) => c.method === 'POST' && c.url.includes(WIRE.TOOLS_EXECUTE));
    assert.equal(posted.length, 1, 'POST /tools/execute must reach the server');

    /**
     * B) client-side: tool server tak ada di union registry → denial sebelum POST.
     */
    runtime = await createAgentRuntime({ serverUrl: url, maxSteps: 5 });
    await runtime.connect();
    await assert.rejects(
      () => runtime.runTurn('use srv_echo'),
      (err) => {
        assert.match(String(err.message), /has no owner in the union registry|permission:/);
        return true;
      },
    );

    /**
     * C) GET /tools binary → array kosong (registri server kosong).
     */
    const defs = await transport.pullTools();
    assert.deepEqual(defs, []);
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    proc.kill();
  }
});

/**
 * T2c — KOREKSI U12b #4: server tool via BINARY NYATA (--server-tool).
 */

test('U12b#2c core@client -> tool server via BINARY NYATA: --server-tool terdaftar, POST /tools/execute dipanggil, hasil masuk session, final', async () => {
  /**
   * Registrasi tool server hidup di binary: `--server-tool server_echo:{"ok":true}`. LLM stub
   * binary statis (call_tool terus-menerus); commit dituntun ke final oleh hook decide-action
   * client (passthrough dulu, override final di iterasi 2) — hook client adalah fitur nyata
   * runtime (T9), bukan simulasi.
   */
  const { proc, url } = await spawnRealServer({
    stubResponse: '{"action":"call_tool","tool":"server_echo","input":{}}',
    clientId: 'server-tool-c1',
    extraArgs: ['--server-tool', 'server_echo:{"ok":true}'],
  });
  const probe = installFetchProbe();
  let runtime = null;
  try {
    let hookCalls = 0;
    runtime = await createAgentRuntime({
      serverUrl: url,
      clientId: 'server-tool-c1',
      maxSteps: 5,
      hooks: {
        decideAction: () => {
          hookCalls += 1;
          if (hookCalls === 1) return { passthrough: true };
          return { action: 'final', content: 'server-tool-final' };
        },
      },
    });
    await runtime.connect();

    /**
     * Daftarkan tool `server_echo` owner server di client registry (GET /tools binary).
     */
    const registry = await runtime.refreshTools();
    assert.equal(registry.ownerOf('server_echo'), 'server');

    const result = await runtime.runTurn('call server_echo');

    assert.equal(result.action, 'final');
    assert.equal(result.content, 'server-tool-final');

    /**
     * POST /tools/execute benar-benar mendarat di binary dengan shape golden.
     */
    const executed = probe.calls.filter((c) => c.method === 'POST' && c.url.includes(WIRE.TOOLS_EXECUTE));
    assert.equal(executed.length, 1, 'server tool must be executed via POST /tools/execute');
    assert.equal(executed[0].body['tool-name'], 'server_echo');
    assert.equal(executed[0].body['arguments-json'], JSON.stringify({}));

    /**
     * Hasil masuk session: event tool_result ada di drain.
     */
    const toolResultEvent = result.events.find((e) => e.kind === 'tool_result');
    assert.ok(toolResultEvent, 'drained events must contain tool_result');
    assert.equal(toolResultEvent.payload.tool, 'server_echo');
    assert.equal(toolResultEvent.payload.success, true);
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    proc.kill();
  }
});

test('U12b#2d binary Rust: GET /tools berisi definisi tool dari --server-tool (boleh berulang)', async () => {
  const { proc, url } = await spawnRealServer({
    stubResponse: STUB_FINAL,
    extraArgs: [
      '--server-tool', 'server_echo:{"ok":true}',
      '--server-tool', 'server_other:{"x":1}',
    ],
  });
  try {
    const transport = createTransport({ serverUrl: url });
    const defs = await transport.pullTools();

    /**
     * Definisi auto-derive: name + description tetap + input_schema object kosong.
     */
    const echo = defs.find((d) => d.name === 'server_echo');
    assert.ok(echo, 'server_echo definition must be present in GET /tools');
    assert.equal(echo.description, 'Server tool registered via --server-tool');
    assert.deepEqual(echo.input_schema, { type: 'object', properties: {}, required: [] });

    const other = defs.find((d) => d.name === 'server_other');
    assert.ok(other, 'repeated --server-tool must register every tool');
    assert.equal(other.description, 'Server tool registered via --server-tool');
  } finally {
    proc.kill();
  }
});

/**
 * T3 — LLM proxy (R6): semua LLM mendarat di serverUrl, tanpa host lain.
 */

test('U12b#3 LLM proxy (R6): seluruh panggilan LLM mendarat di POST /llm/call server; content commit == stub', async () => {
  const { proc, url } = await spawnRealServer({ stubResponse: STUB_FINAL });
  const probe = installFetchProbe();
  let runtime = null;
  try {
    runtime = await createAgentRuntime({ serverUrl: url, maxSteps: 5 });
    await runtime.connect();
    const result = await runtime.runTurn('hello proxy');

    /**
     * Content commit == stub response.
     */
    assert.equal(result.action, 'final');
    assert.equal(result.content, 'stub-final');

    /**
     * SEMUA fetch client menuju host serverUrl saja.
     */
    assert.ok(probe.calls.length > 0, 'expected client fetches to be observed');
    const serverHost = new URL(url).host;
    for (const call of probe.calls) {
      assert.equal(new URL(call.url).host, serverHost, `client fetched foreign host: ${call.url}`);
      assert.ok(call.url.includes('/antikythera/v1/'), `client fetched non-wire path: ${call.url}`);
    }

    const llmPosts = probe.calls.filter((c) => c.method === 'POST' && c.url.includes(WIRE.LLM_CALL));
    assert.ok(llmPosts.length >= 1, 'expected POST /llm/call');

    /**
     * Wire shape request yang benar-benar dikirim.
     */
    const body = llmPosts[0].body;
    assertWireObject(body, golden().llm_call_request, 'llm_call_request (T3)', [
      'provider', 'model', 'session_id', 'messages_json', 'force_json', 'temperature', 'max_tokens', 'schema_name', 'metadata_json',
    ]);
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    proc.kill();
  }
});


test('U12b#4a streaming: server Rust mendukung token via SSE (POST /llm/call?stream=true -> llm-token)', async () => {
  const { proc, url } = await spawnRealServer({
    stubResponse: '{"action":"final","content":"stream-me"}',
    clientId: 'stream-c1',
  });
  let reader = null;
  try {
    const sseRes = await fetch(`${url}${WIRE.EVENTS}?client_id=stream-c1`);
    assert.equal(sseRes.status, 200);
    reader = sseRes.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    const frames = [];
    const tokenArrived = new Promise((resolve) => {
      const pump = async () => {
        const { done, value } = await reader.read();
        if (done) return resolve();
        buffer += decoder.decode(value, { stream: true });
        const parts = buffer.split('\n\n');
        buffer = parts.pop() ?? '';
        for (const frame of parts) frames.push(frame);
        if (frames.some((f) => f.includes('llm-token'))) return resolve();
        return pump();
      };
      pump();
    });

    const llmRes = await fetch(`${url}${WIRE.LLM_CALL}?stream=true`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        provider: null, model: 'stub', session_id: 's-stream', messages_json: '[]',
        force_json: false, temperature: null, max_tokens: null, schema_name: null, metadata_json: '{}',
      }),
    });
    assert.equal(llmRes.status, 200);

    await waitFor(() => frames.some((f) => f.includes('llm-token')), { timeoutMs: 5000, label: 'llm-token frame' });
    const tokenFrame = frames.find((f) => f.includes('llm-token'));
    const dataLine = tokenFrame.split('\n').find((l) => l.startsWith('data:'));
    const env = JSON.parse(dataLine.slice(5));

    assertWireObject(env, golden().llm_token_event, 'llm_token_event (T4a)', ['type', 'correlation_id', 'session_id', 'client_id', 'payload']);
    assert.equal(env.type, 'llm-token');
    assert.equal(env.payload.chunk, '{"action":"final","content":"stream-me"}');
    assert.equal(env.session_id, 's-stream');
    assert.equal(env.client_id, 'stream-c1');
  } finally {
    if (reader) reader.cancel().catch(() => {});
    proc.kill();
  }
});

test('U12b#4b runTurn streaming: client mengirim ?stream=true sendiri, token ter-feed ke runner, commit deterministik', async () => {
  /**
   * KOREKSI U12b: sinyal streaming hidup di QUERY PARAM `?stream=true` (transport.llmCall opsi
   * stream), BUKAN di metadata_json — tidak ada lagi fetch-patch. Commit stream deterministik:
   * setelah POST /llm/call selesai, runtime menunggu token SSE mereda sebelum memutuskan
   * commitLlmStream.
   */
  const { proc, url } = await spawnRealServer({
    stubResponse: '{"action":"final","content":"stream-final"}',
    clientId: 'stream-c1',
  });
  const probe = installFetchProbe();
  let runtime = null;
  try {
    runtime = await createAgentRuntime({ serverUrl: url, clientId: 'stream-c1', maxSteps: 5 });
    const tokenEvents = [];
    const fallbackEvents = [];
    runtime.onEvent((e) => { if (e.type === 'llm-token') tokenEvents.push(e); });
    runtime.onEvent((e) => { if (e.type === 'stream-fallback') fallbackEvents.push(e); });
    await runtime.connect();
    const result = await runtime.runTurn('stream me');

    assert.equal(result.content, 'stream-final');

    /**
     * KOREKSI 1: klien menambahkan ?stream=true pada POST /llm/call.
     */
    const llmPost = probe.calls.find((c) => c.method === 'POST' && c.url.includes(WIRE.LLM_CALL));
    assert.ok(llmPost, 'POST /llm/call must be observed');
    assert.ok(
      new URL(llmPost.url).searchParams.get('stream') === 'true',
      `streaming must be signaled by ?stream=true, got URL: ${llmPost.url}`,
    );
    assert.equal(JSON.parse(llmPost.body.metadata_json ?? '{}').stream, undefined, 'stream flag must not live in metadata_json');

    /**
     * KOREKSI 2: jalur streaming (bukan fallback) — token SSE nyata ter-feed.
     */
    assert.equal(fallbackEvents.length, 0, 'no stream-fallback expected on a live stream');
    await waitFor(() => tokenEvents.length > 0, { timeoutMs: 5000, label: 'llm-token runtime event' });
    assert.equal(tokenEvents[0].chunk, '{"action":"final","content":"stream-final"}');
    assert.equal(tokenEvents[0].sessionId, runtime.sessionId);
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    proc.kill();
  }
});

test('U12b#4c alur non-streaming default (tanpa patch) tetap benar', async () => {
  const { proc, url } = await spawnRealServer({ stubResponse: STUB_FINAL });
  let runtime = null;
  try {
    runtime = await createAgentRuntime({ serverUrl: url, maxSteps: 5 });
    await runtime.connect();
    const result = await runtime.runTurn('plain');
    assert.equal(result.action, 'final');
    assert.equal(result.content, 'stub-final');
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    proc.kill();
  }
});

/**
 * T5 — Registry sync pull.
 */

test('U12b#5 registry sync pull: union = tool server (GET /tools) + tool client; refreshTools single-call', async () => {
  const server = await new FakeWireServer({
    serverTools: [
      makeToolEntry('srv_a', 'server a'),
      makeToolEntry('srv_b', 'server b'),
    ],
    llmResponder: () => ({ content: STUB_FINAL }),
  }).start();

  const registryEvents = [];
  let runtime = null;
  try {
    runtime = await createAgentRuntime({
      serverUrl: server.url(),
      tools: [makeToolEntry('cli_a', 'client a', async () => 'a')],
      policy: { allow: ['cli_a'] },
      maxSteps: 5,
    });
    runtime.onEvent((e) => { if (e.type === 'registry') registryEvents.push(e); });
    await runtime.connect();
    /**
     * Connect melakukan tepat satu pull GET /tools.
     */
    assert.equal(server.getToolsCalls, 1, 'connect must pull GET /tools exactly once');

    const registry = await runtime.refreshTools();
    /**
     * Union berisi tool server + tool client.
     */
    assert.equal(registry.size(), 3);
    assert.equal(registry.ownerOf('srv_a'), 'server');
    assert.equal(registry.ownerOf('srv_b'), 'server');
    assert.equal(registry.ownerOf('cli_a'), 'client');
    const names = registry.toDefinitions().map((d) => d.name).sort();
    assert.deepEqual(names, ['cli_a', 'srv_a', 'srv_b']);

    /**
     * RefreshTools = satu pull lagi.
     */
    assert.equal(server.getToolsCalls, 2, 'refreshTools must pull exactly once more');

    /**
     * Event registry ter-emit dengan count union.
     */
    assert.ok(registryEvents.length >= 1, 'registry event expected');
    assert.equal(registryEvents[0].count, 3);
    assert.equal(registryEvents[0].owners.cli_a, 'client');
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    await server.close();
  }
});

test('U12b#5b binary Rust: GET /tools kosong -> union hanya tool client', async () => {
  const { proc, url } = await spawnRealServer({ stubResponse: STUB_FINAL });
  let runtime = null;
  try {
    runtime = await createAgentRuntime({
      serverUrl: url,
      tools: [makeToolEntry('cli_only', 'client only', async () => 'x')],
      policy: { allow: ['cli_only'] },
      maxSteps: 5,
    });
    await runtime.connect();
    const registry = await runtime.refreshTools();
    assert.equal(registry.size(), 1);
    assert.equal(registry.ownerOf('cli_only'), 'client');
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    proc.kill();
  }
});


test('U12b#6 collision: tool nama sama di client dan server -> error eksplisit saat connect/refresh', async () => {
  const server = await new FakeWireServer({
    serverTools: [makeToolEntry('dup_tool', 'server side dup')],
    llmResponder: () => ({ content: STUB_FINAL }),
  }).start();

  try {
    let runtime = null;
    try {
      runtime = await createAgentRuntime({
        serverUrl: server.url(),
        tools: [makeToolEntry('dup_tool', 'client side dup', async () => 'x')],
        policy: { allow: ['dup_tool'] },
        maxSteps: 5,
      });
      await assert.rejects(
        () => runtime.connect(),
        (err) => {
          assert.match(String(err.message), /collision/);
          assert.match(String(err.message), /dup_tool/);
          return true;
        },
      );
    } finally {
      if (runtime) runtime.close();
    }
  } finally {
    clearRuntimeHooksProvider();
    await server.close();
  }
});

test('U12b#6b createUnionRegistry: unit-level collision dan non-collision', () => {
  const dupDef = makeToolDef('same_name', 'dup');
  assert.throws(
    () => createUnionRegistry({
      localEntries: [{ definition: dupDef, handler: () => 'x' }],
      serverDefinitions: [dupDef],
    }),
    /collision/,
  );
  const union = createUnionRegistry({
    localEntries: [{ definition: makeToolDef('client_t', 'c'), handler: () => 'x' }],
    serverDefinitions: [makeToolDef('server_t', 's')],
  });
  assert.equal(union.size(), 2);
  assert.equal(union.ownerOf('client_t'), 'client');
  assert.equal(union.ownerOf('server_t'), 'server');
});

/**
 * T7 — Permission (policy client default-deny).
 */

test('U12b#7 permission: tool client yang tidak di-allow -> denial permission:', async () => {
  const { proc, url } = await spawnRealServer({
    stubResponse: '{"action":"call_tool","tool":"secret_tool","input":{}}',
  });
  let runtime = null;
  try {
    runtime = await createAgentRuntime({
      serverUrl: url,
      tools: [makeToolEntry('secret_tool', 'secret', async () => ({ leaked: true }))],
      policy: { allow: [] },
      maxSteps: 5,
    });
    await runtime.connect();

    /**
     * RunTurn: LLM minta call_tool secret_tool → gate menolak.
     */
    await assert.rejects(
      () => runtime.runTurn('use the secret tool'),
      (err) => { assertPermissionError(err, 'runTurn gate denial'); return true; },
    );

    await assert.rejects(
      () => runtime.executeTool('secret_tool', {}),
      (err) => { assertPermissionError(err, 'executeTool gate denial'); return true; },
    );
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    proc.kill();
  }
});

test('U12b#7b permission positive control: tool yang di-allow dieksekusi', async () => {
  const { proc, url } = await spawnRealServer({ stubResponse: STUB_FINAL });
  let runtime = null;
  try {
    runtime = await createAgentRuntime({
      serverUrl: url,
      tools: [makeToolEntry('allowed_tool', 'allowed', async (args) => ({ echoed: args }))],
      policy: { allow: ['allowed_tool'] },
      maxSteps: 5,
    });
    await runtime.connect();
    const result = await runtime.executeTool('allowed_tool', { q: 1 });
    assert.equal(result.success, true);
    assert.equal(result.output_json, JSON.stringify({ echoed: { q: 1 } }));
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    proc.kill();
  }
});

/**
 * T8 — Wire-shape consistency (invarian 5).
 */

test('U12b#8 wire-shape consistency: sample request/response nyata subset dari golden', async () => {
  const server = await new FakeWireServer({
    serverTools: [makeToolEntry('srv_wire', 'wire server tool', (args) => ({ out: args }))],
    allowServerTools: ['srv_wire'],
    llmResponder: (index) => ({
      content: index === 0
        ? '{"action":"call_tool","tool":"srv_wire","input":{"k":"v"}}'
        : '{"action":"final","content":"wire-final"}',
      model: 'stub', session_id: null, message_json: null, tokens_used: 3, finish_reason: 'stop', raw_response_json: null,
    }),
  }).start();

  let runtime = null;
  let peer = null;
  let peerServer = null;
  let binaryProc = null;
  try {
    runtime = await createAgentRuntime({
      serverUrl: server.url(),
      tools: [makeToolEntry('cli_wire', 'wire client tool', async () => ({ local: true }))],
      policy: { allow: ['cli_wire'] },
      maxSteps: 5,
    });
    await runtime.connect();
    await runtime.runTurn('wire me');

    assert.ok(server.llmCalls.length >= 1);
    assertWireObject(server.llmCalls[0], golden().llm_call_request, 'llm_call_request', [
      'provider', 'model', 'session_id', 'messages_json', 'force_json', 'temperature', 'max_tokens', 'schema_name', 'metadata_json',
    ]);
    /**
     * Llm-response yang benar-benar dikirim server.
     */
    assertWireObject(server.llmResponses[0], golden().llm_call_response, 'llm_call_response', [
      'content', 'model', 'session_id', 'message_json', 'tokens_used', 'finish_reason', 'raw_response_json',
    ]);
    assert.equal(server.toolExecutes.length, 1);
    assertWireObject(server.toolExecutes[0], golden().tool_execute_request, 'tool_execute_request', [
      'tool-name', 'arguments-json', 'session-id', 'step-id',
    ]);
    /**
     * Tools_list_response: definisi tool server subset golden.
     */
    await runtime.refreshTools();
    assert.ok(server.getToolsCalls >= 2);
    const goldenDef = golden().tools_list_response[0];
    for (const [name, entry] of server.serverTools) {
      assertNoExtraKeys(entry.definition, goldenDef, `tool definition '${name}'`);
    }
    runtime.close();
    runtime = null;

    peerServer = await new FakeWireServer({}).start();
    const postbacks = peerServer.postbacks;
    peer = await createAgentRuntime({
      core: 'server',
      serverUrl: peerServer.url(),
      clientId: 'wire-peer',
      tools: [makeToolEntry('cli_wire', 'wire client tool', async () => ({ local: true }))],
      policy: { allow: ['cli_wire'] },
    });
    await peer.connect();
    await waitFor(() => peerServer.sseClients.has('wire-peer'), { timeoutMs: 5000, label: 'peer SSE register' });

    const hookPayload = { hook: 'decide-action', session_state_json: '{}', input_json: '{}' };
    const toolPayload = { 'tool-name': 'cli_wire', 'arguments-json': '{}', 'session-id': 's-wire', 'step-id': 3 };
    peerServer.pushEnvelope('wire-peer', {
      type: 'tool-execution-request', correlation_id: 'corr-w-t', session_id: 's-wire', client_id: 'wire-peer', payload: toolPayload,
    });
    peerServer.pushEnvelope('wire-peer', {
      type: 'hook-request', correlation_id: 'corr-w-h', session_id: 's-wire', client_id: 'wire-peer', payload: hookPayload,
    });
    await waitFor(() => postbacks.length >= 2, { timeoutMs: 5000, label: 'peer postbacks' });

    const goldenToolEvent = golden().tool_execution_request_event;
    assertNoExtraKeys(
      { type: 'tool-execution-request', correlation_id: 'corr-w-t', session_id: 's-wire', client_id: 'wire-peer', payload: toolPayload },
      goldenToolEvent,
      'tool_execution_request_event',
    );
    const goldenHookEvent = golden().hook_request_event;
    assertNoExtraKeys(
      { type: 'hook-request', correlation_id: 'corr-w-h', session_id: 's-wire', client_id: 'wire-peer', payload: hookPayload },
      goldenHookEvent,
      'hook_request_event',
    );

    const toolPostback = postbacks.find((p) => p.correlation_id === 'corr-w-t');
    assert.ok(toolPostback, 'tool postback expected');
    assertWireObject(toolPostback, golden().postback_response, 'postback_response', ['correlation_id', 'ok', 'payload', 'error']);
    assertWireObject(toolPostback.payload, golden().tool_execute_response, 'tool_execute_response (postback payload)', [
      'tool-name', 'success', 'output-json', 'error-message', 'step-id',
    ]);

    const hookPostback = postbacks.find((p) => p.correlation_id === 'corr-w-h');
    assert.ok(hookPostback, 'hook postback expected');
    assertWireObject(hookPostback, golden().postback_response, 'hook postback_response', ['correlation_id', 'ok', 'payload', 'error']);
    peer.close();
    peer = null;
    await peerServer.close();
    peerServer = null;

    const spawned = await spawnRealServer({ stubResponse: STUB_FINAL });
    binaryProc = spawned.proc;
    const binUrl = spawned.url;
    const llmRes = await fetch(`${binUrl}${WIRE.LLM_CALL}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        provider: null, model: 'stub', session_id: 's-w', messages_json: '[]',
        force_json: false, temperature: null, max_tokens: null, schema_name: null, metadata_json: '{}',
      }),
    });
    assert.equal(llmRes.status, 200);
    const llmBody = await llmRes.json();
    assertWireObject(llmBody, golden().llm_call_response, 'llm_call_response (binary)', [
      'content', 'model', 'session_id', 'message_json', 'tokens_used', 'finish_reason', 'raw_response_json',
    ]);

    /**
     * Gate denial server: {"error": "permission: ..."}.
     */
    const transport = createTransport({ serverUrl: binUrl });
    await assert.rejects(
      () => transport.executeServerTool(buildToolCallEvent({ toolName: 'nope', argumentsJson: '{}', sessionId: 's', stepId: 0 })),
      (err) => {
        assertPermissionError(err, 'binary gate denial');
        return true;
      },
    );
    const errRes = await fetch(`${binUrl}${WIRE.TOOLS_EXECUTE}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(buildToolCallEvent({ toolName: 'nope', argumentsJson: '{}', sessionId: 's', stepId: 0 })),
    });
    assert.equal(errRes.status, 403);
    const errBody = await errRes.json();
    assertNoExtraKeys(errBody, golden().error_event.payload, 'gate denial error body');
  } finally {
    if (runtime) runtime.close();
    if (peer) peer.close();
    if (peerServer) await peerServer.close();
    if (binaryProc) binaryProc.kill();
    clearRuntimeHooksProvider();
    await server.close();
  }
});

/**
 * T9 — Hook decision client (in-process runtime-hooks).
 */

test('U12b#9 hook decision client: provider override decideAction -> content override dipakai', async () => {
  const { proc, url } = await spawnRealServer({ stubResponse: STUB_FINAL });
  let runtime = null;
  try {
    let hookCalls = 0;
    runtime = await createAgentRuntime({
      serverUrl: url,
      hooks: {
        decideAction: (stateJson, llmJson) => {
          hookCalls += 1;
          /**
           * Arg order: (session_state, llm_response) — kontrak WIT runtime-hooks.
           */
          assert.equal(typeof stateJson, 'string');
          assert.equal(typeof llmJson, 'string');
          return { content: 'hook-override' };
        },
      },
      maxSteps: 5,
    });
    await runtime.connect();
    const result = await runtime.runTurn('override me');
    assert.equal(result.action, 'final');
    assert.equal(result.content, 'hook-override', 'hook decision must replace stub content');
    assert.ok(hookCalls >= 1, 'decideAction must be invoked in-process');
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    proc.kill();
  }
});

test('U12b#9b hook failure fail-closed: decideAction throws -> runTurn gagal (bukan passthrough)', async () => {
  const { proc, url } = await spawnRealServer({ stubResponse: STUB_FINAL });
  let runtime = null;
  try {
    runtime = await createAgentRuntime({
      serverUrl: url,
      hooks: {
        decideAction: () => {
          throw new Error('boom-decision');
        },
      },
      maxSteps: 5,
    });
    await runtime.connect();
    await assert.rejects(
      () => runtime.runTurn('force failure'),
      (err) => {
        const message = err instanceof Error ? err.message : String(err);
        assert.match(message, /boom-decision|decide-action|runtime-hook/i);
        return true;
      },
    );
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    proc.kill();
  }
});

test('U12b#9c default passthrough: tanpa hooks provider, content stub dipakai apa adanya', async () => {
  const { proc, url } = await spawnRealServer({ stubResponse: STUB_FINAL });
  let runtime = null;
  try {
    runtime = await createAgentRuntime({ serverUrl: url, maxSteps: 5 });
    await runtime.connect();
    const result = await runtime.runTurn('plain');
    assert.equal(result.content, 'stub-final');
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
    proc.kill();
  }
});

/**
 * T9d — KOREKSI U12b #3: pembersihan global hooks provider.
 */

test('U12b#9d control: installRuntimeHooksProvider(null) MENGHAPUS global provider', async () => {
  const { installRuntimeHooksProvider } = rt;
  try {
    installRuntimeHooksProvider({ decideAction: () => '{"action":"final","content":"x"}' });
    assert.ok(
      globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__,
      'provider must be installed before the null call',
    );
    installRuntimeHooksProvider(null);
    assert.equal(
      globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__,
      undefined,
      'null install must DELETE the global provider, not leave it',
    );
  } finally {
    clearRuntimeHooksProvider();
  }
});

test('U12b#9e runtime owns hooks without writing the legacy global provider', async () => {
  let runtime = null;
  try {
    runtime = await createAgentRuntime({
      serverUrl: 'http://127.0.0.1:1',
      hooks: { decideAction: () => '{"action":"final","content":"x"}' },
    });
    assert.equal(globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__, undefined, 'instance imports must not publish a global provider');
    runtime.close();
    runtime = null;
    assert.equal(
      globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__,
      undefined,
      'runtime.close() must delete the global provider',
    );
  } finally {
    if (runtime) runtime.close();
    clearRuntimeHooksProvider();
  }
});

/**
 * T10 — core@server peer (arah server→client dari sisi client).
 */

test('U12b#10 core@server peer: tool-execution-request + hook-request dijawab via POST-back', async () => {
  const server = await new FakeWireServer({}).start();
  const postbacks = server.postbacks;
  let handlerCalls = 0;
  let peer = null;

  try {
    peer = await createAgentRuntime({
      core: 'server',
      serverUrl: server.url(),
      clientId: 'peer-1',
      tools: [makeToolEntry('peer_echo', 'peer echo', async (args) => {
        handlerCalls += 1;
        return { echoed: args };
      })],
      policy: { allow: ['peer_echo'] },
      hooks: {
        decideAction: () => '{"action":"final","content":"peer-hook"}',
      },
    });
    await peer.connect();
    await waitFor(() => server.sseClients.has('peer-1'), { timeoutMs: 5000, label: 'peer SSE registration' });

    /**
     * Server meminta tool client (payload = tool-call-event).
     */
    server.pushEnvelope('peer-1', {
      type: 'tool-execution-request',
      correlation_id: 'corr-t1',
      session_id: 's1',
      client_id: 'peer-1',
      payload: { 'tool-name': 'peer_echo', 'arguments-json': '{"x":1}', 'session-id': 's1', 'step-id': 2 },
    });
    server.pushEnvelope('peer-1', {
      type: 'hook-request',
      correlation_id: 'corr-h1',
      session_id: 's1',
      client_id: 'peer-1',
      payload: { hook: 'decide-action', session_state_json: '{}', input_json: '{}' },
    });
    /**
     * Tool client yang TIDAK di-allowlist → denial permission:
     */
    server.pushEnvelope('peer-1', {
      type: 'tool-execution-request',
      correlation_id: 'corr-d1',
      session_id: 's1',
      client_id: 'peer-1',
      payload: { 'tool-name': 'denied_tool', 'arguments-json': '{}', 'session-id': 's1', 'step-id': 1 },
    });

    await waitFor(() => postbacks.length >= 3, { timeoutMs: 5000, label: 'peer postbacks' });

    /**
     * A) tool execution POST-back: ok=true, payload shape golden tool_execute_response.
     */
    const t1 = postbacks.find((p) => p.correlation_id === 'corr-t1');
    assert.ok(t1, 'tool postback missing');
    assert.equal(t1.ok, true);
    assert.equal(handlerCalls, 1, 'client handler must run once');
    assertWireObject(t1, golden().postback_response, 'postback_response (tool)', ['correlation_id', 'ok', 'payload', 'error']);
    assertWireObject(t1.payload, golden().tool_execute_response, 'tool_execute_response (postback)', [
      'tool-name', 'success', 'output-json', 'error-message', 'step-id',
    ]);
    assert.equal(t1.payload['tool-name'], 'peer_echo');
    assert.equal(t1.payload.success, true);
    assert.equal(t1.payload['output-json'], JSON.stringify({ echoed: { x: 1 } }));
    assert.equal(t1.payload['step-id'], 2);

    /**
     * B) hook decision POST-back: payload = decision string.
     */
    const h1 = postbacks.find((p) => p.correlation_id === 'corr-h1');
    assert.ok(h1, 'hook postback missing');
    assert.equal(h1.ok, true);
    assert.equal(h1.payload, '{"action":"final","content":"peer-hook"}');

    /**
     * C) denial: ok=false, error berawalan permission:
     */
    const d1 = postbacks.find((p) => p.correlation_id === 'corr-d1');
    assert.ok(d1, 'denial postback missing');
    assert.equal(d1.ok, false);
    assert.ok(String(d1.error).startsWith('permission:'), `expected permission: denial, got ${d1.error}`);
    assertNoExtraKeys(d1, golden().postback_gate_denial, 'postback_gate_denial');
  } finally {
    if (peer) peer.close();
    clearRuntimeHooksProvider();
    await server.close();
  }
});

test('U12b#10b binary Rust: endpoint POST-back menerima respons dan menolak client_id kosong di SSE', async () => {
  const { proc, url } = await spawnRealServer({ stubResponse: STUB_FINAL });
  try {
    /**
     * POST-back dengan correlation id tak dikenal → 204, diabaikan (bukan error).
     */
    const res = await fetch(`${url}${WIRE.EVENTS}/unknown-corr/response`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ correlation_id: 'unknown-corr', ok: true, payload: null, error: null }),
    });
    assert.equal(res.status, 204);

    /**
     * GET /events tanpa client_id → 400 (query REQUIRED).
     */
    const noClient = await fetch(`${url}${WIRE.EVENTS}`);
    assert.equal(noClient.status, 400);
  } finally {
    proc.kill();
  }
});
