/**
 * Parity-python-server.test.mjs — U61: server Python NYATA adalah drop-in peer wire-protocol
 * (keputusan D2). Suite ini MIRROR `runtime-bridge.test.mjs` (lane binary Rust) untuk jalur yang
 * didukung CLI Python, membuktikan bahwa client runtime NYATA tidak dapat membedakan server Rust
 * dan server Python: respon wire identik golden, denial fail-closed, streaming deterministik.
 * Kontrak acuan: - documentation/WIRE_PROTOCOL.md (endpoint §2, envelope §3, streaming §4,
 * timeout §5, mapping §6) - contracts/shared/wire_protocol.golden.json -
 * clients/npm/antikythera-sdk/runtime/** (client runtime ASLI) -
 * clients/python/antikythera_agent/server/__main__.py (CLI U32: `--bind`, `--provider-stub`,
 * `--server-tool`, `--allow-tool`, `--client-id`, baris `[server-runtime] HTTP wire bridge
 * listening on <url>`) Peta falsifikasi U61 (mirror runtime-bridge.test.mjs untuk jalur CLI
 * Python): PARITY#1  LLM proxy: seluruh panggilan LLM mendarat di POST /llm/call server Python;
 * content commit == stub; wire shape golden. PARITY#2  Tool server: `--server-tool` terdaftar ->
 * GET /tools berisi definisi golden; POST /tools/execute dipanggil; hasil masuk session; final
 * (hook decideAction memandu iterasi 2). PARITY#3  Deny fail-closed: tool server tak terdaftar
 * -> POST /tools/execute 403 `permission:`; runTurn tool tanpa owner -> error union registry.
 * PARITY#4  Streaming: `?stream=true` -> llm-token event dari server Python; runTurn streaming
 * commit deterministik (tanpa stream-fallback). PARITY#5  Permission: tool client tidak di-allow
 * -> denial `permission:`. PARITY#6  Wire-shape: sample request/response nyata terhadap server
 * Python subset golden (llm_call_request, llm_call_response, tool_execute_request,
 * tool_execute_response, tools_list_response). PARITY#7  Hook decision client: hooks provider
 * override bekerja melalui server Python (content override dikomit). Batas yang DIDOKUMENTASIKAN
 * (CLI Python vs binary Rust): - CLI Python tidak mendukung MCP connect / `--smoke` / scripted
 * LLM stateful. Skenario yang butuh dua LLM response (call_tool -> final) memakai hook
 * decideAction client untuk memandu iterasi 2 (pola U12b#2c) — bukan LLM kedua.
 */

'use strict';

import net from 'node:net';
import path from 'node:path';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { test, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';

import rt from '../../clients/npm/antikythera-sdk/runtime/index.js';

const { createAgentRuntime, createTransport, WIRE, buildToolCallEvent } = rt;


const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, '..', '..');
const PYTHON_SRC = path.join(REPO_ROOT, 'clients', 'python');
const GOLDEN_PATH = path.join(REPO_ROOT, 'contracts', 'shared', 'wire_protocol.golden.json');

const STUB_FINAL = '{"action":"final","content":"stub-final"}';

import { PYTHON_CMD } from './python-executable.mjs';


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

function assertPermissionError(err, label) {
  const message = err instanceof Error ? err.message : String(err);
  assert.ok(message.startsWith('permission:'), `${label}: expected permission: denial, got: ${message}`);
}

/**
 * Process / port helpers (pola runtime-bridge.test.mjs / component-base-live).
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

function pythonEnv() {
  const parts = [PYTHON_SRC, process.env.PYTHONPATH].filter(Boolean);
  return { ...process.env, PYTHONPATH: parts.join(path.delimiter) };
}

const activeSpawns = new Set();
const activeRuntimes = new Set();

async function terminateProc(proc, { timeoutMs = 4000 } = {}) {
  if (!proc || proc.exitCode !== null || proc.signalCode !== null) return;
  try { proc.stdout?.destroy(); } catch { }
  try { proc.stderr?.destroy(); } catch { }
  /**
   * Destroy stdio pipes to unblock event loop; keep kill semantics.
   */
  try { proc.kill(); } catch { }
  const exited = await Promise.race([
    new Promise((resolve) => proc.once('exit', () => resolve(true))),
    new Promise((resolve) => setTimeout(() => resolve(false), timeoutMs)),
  ]);
  if (!exited) {
    try { proc.kill('SIGKILL'); } catch { }
    await Promise.race([
      new Promise((resolve) => proc.once('exit', () => resolve())),
      new Promise((resolve) => setTimeout(resolve, 2000)),
    ]);
  }
  try { proc.stdout?.destroy(); } catch { }
  try { proc.stderr?.destroy(); } catch { }
  try { proc.stdin?.destroy(); } catch { }
}

function trackRuntime(rt) {
  if (rt) activeRuntimes.add(rt);
  return rt;
}
function untrackRuntime(rt) {
  if (rt) activeRuntimes.delete(rt);
}

/**
 * Spawn `python -m antikythera_agent.server` (U32 CLI) dan tunggu baris
 * listening (timeout 15s). Mengembalikan proc/port/url/line.
 * @param {object} opts
 * @param {string} [opts.stubResponse]
 * @param {string} [opts.clientId]
 * @param {Array<string>} [opts.serverTools]  — `name:json` pairs
 * @param {Array<string>} [opts.allowTools]
 */
function spawnPythonServer({ stubResponse = STUB_FINAL, clientId = null, serverTools = [], allowTools = [] } = {}) {
  return new Promise((resolve, reject) => {
    freePort().then((port) => {
      const args = ['-m', 'antikythera_agent.server', '--bind', `127.0.0.1:${port}`, '--provider-stub', stubResponse];
      if (clientId) args.push('--client-id', clientId);
      for (const tool of serverTools) args.push('--server-tool', tool);
      for (const tool of allowTools) args.push('--allow-tool', tool);
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


function clearRuntimeHooksProvider() {
  delete globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__;
}

beforeEach(() => {
  clearRuntimeHooksProvider();
});

afterEach(async () => {
  restoreAllFetchProbes();
  clearRuntimeHooksProvider();
  /**
   * Tutup SSE channel dulu sebelum kill server — urutan deterministik mencegah reconnect timer
   * menahan event loop (RISIKO-01).
   */
  for (const rt of [...activeRuntimes]) {
    try { rt.close(); } catch { }
  }
  activeRuntimes.clear();
  const kills = [...activeSpawns].map((p) => terminateProc(p));
  await Promise.allSettled(kills);
  activeSpawns.clear();
});

/**
 * Tool definition factory (shape golden tools_list_response).
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

/**
 * PARITY#1 — LLM proxy (R6): semua LLM mendarat di POST /llm/call Python.
 */

test('PARITY#1 LLM proxy: seluruh panggilan LLM mendarat di POST /llm/call server Python; content commit == stub; wire shape golden', async () => {
  const { proc, url } = await spawnPythonServer({ stubResponse: STUB_FINAL });
  const probe = installFetchProbe();
  let runtime = null;
  try {
    runtime = await createAgentRuntime({ serverUrl: url, maxSteps: 5 });
    trackRuntime(runtime);
    await runtime.connect();
    const result = await runtime.runTurn('hello python proxy');

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
    assertWireObject(body, golden().llm_call_request, 'llm_call_request (PARITY#1)', [
      'provider', 'model', 'session_id', 'messages_json', 'force_json', 'temperature', 'max_tokens', 'schema_name', 'metadata_json',
    ]);
  } finally {
    if (runtime) { try { runtime.close(); } catch {} untrackRuntime(runtime); }
    clearRuntimeHooksProvider();
    await terminateProc(proc);
    activeSpawns.delete(proc);
  }
});

/**
 * PARITY#2 — Tool server via --server-tool.
 */

test('PARITY#2 core@client -> tool server via server Python NYATA: --server-tool terdaftar, POST /tools/execute dipanggil, hasil masuk session, final', async () => {
  const { proc, url } = await spawnPythonServer({
    stubResponse: '{"action":"call_tool","tool":"server_echo","input":{}}',
    clientId: 'parity-tool-c1',
    serverTools: ['server_echo:{"ok":true}'],
  });
  const probe = installFetchProbe();
  let runtime = null;
  try {
    let hookCalls = 0;
    runtime = await createAgentRuntime({
      serverUrl: url,
      clientId: 'parity-tool-c1',
      maxSteps: 5,
      hooks: {
        decideAction: () => {
          hookCalls += 1;
          if (hookCalls === 1) return { passthrough: true };
          return { action: 'final', content: 'parity-server-tool-final' };
        },
      },
    });
    trackRuntime(runtime);
    await runtime.connect();

    /**
     * Daftarkan tool server di client registry (GET /tools binary).
     */
    const registry = await runtime.refreshTools();
    assert.equal(registry.ownerOf('server_echo'), 'server');

    const result = await runtime.runTurn('call server_echo');

    assert.equal(result.action, 'final');
    assert.equal(result.content, 'parity-server-tool-final');

    /**
     * POST /tools/execute benar-benar mendarat di server Python.
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
    if (runtime) { try { runtime.close(); } catch {} untrackRuntime(runtime); }
    clearRuntimeHooksProvider();
    await terminateProc(proc);
    activeSpawns.delete(proc);
  }
});


test('PARITY#3 deny fail-closed: tool server tak terdaftar -> POST /tools/execute 403 permission:; runTurn tool tanpa owner -> error union registry', async () => {
  const { proc, url } = await spawnPythonServer({
    stubResponse: '{"action":"call_tool","tool":"nope_tool","input":{}}',
    allowTools: ['nope_tool'],
  });
  const probe = installFetchProbe();
  let runtime = null;
  try {
    /**
     * (a) POST /tools/execute langsung ke tool tak terdaftar -> 403 permission:
     */
    const transport = createTransport({ serverUrl: url });
    await assert.rejects(
      () => transport.executeServerTool(buildToolCallEvent({ toolName: 'nope_tool', argumentsJson: '{}', sessionId: 's-deny', stepId: 1 })),
      (err) => { assertPermissionError(err, 'python tools/execute deny'); return true; },
    );
    const errRes = await fetch(`${url}${WIRE.TOOLS_EXECUTE}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(buildToolCallEvent({ toolName: 'nope_tool', argumentsJson: '{}', sessionId: 's-deny', stepId: 1 })),
    });
    assert.equal(errRes.status, 403);
    const errBody = await errRes.json();
    assertNoExtraKeys(errBody, golden().error_event.payload, 'gate denial error body');

    /**
     * (b) client-side: tool server tak ada di union registry -> denial sebelum POST.
     */
    runtime = await createAgentRuntime({ serverUrl: url, maxSteps: 5 });
    trackRuntime(runtime);
    await runtime.connect();
    await assert.rejects(
      () => runtime.runTurn('use nope_tool'),
      (err) => {
        assert.match(String(err.message), /has no owner in the union registry|permission:/);
        return true;
      },
    );

    /**
     * (c) GET /tools -> array kosong (registri server kosong tanpa --server-tool).
     */
    const defs = await transport.pullTools();
    assert.deepEqual(defs, []);
  } finally {
    if (runtime) { try { runtime.close(); } catch {} untrackRuntime(runtime); }
    clearRuntimeHooksProvider();
    await terminateProc(proc);
    activeSpawns.delete(proc);
  }
});

/**
 * PARITY#4 — Streaming llm-token dari server Python.
 */

test('PARITY#4 streaming: server Python mendukung token via SSE (POST /llm/call?stream=true -> llm-token) + runTurn commit deterministik', async () => {
  const { proc, url } = await spawnPythonServer({
    stubResponse: '{"action":"final","content":"parity-stream-final"}',
    clientId: 'parity-stream-c1',
  });
  let runtime = null;
  let sseReader = null;
  try {
    /**
     * (a) raw SSE: POST /llm/call?stream=true -> llm-token event.
     */
    const sseRes = await fetch(`${url}${WIRE.EVENTS}?client_id=parity-stream-c1`);
    assert.equal(sseRes.status, 200);
    const reader = sseRes.body.getReader();
    sseReader = reader;
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
        provider: null, model: 'stub', session_id: 's-parity-stream', messages_json: '[]',
        force_json: false, temperature: null, max_tokens: null, schema_name: null, metadata_json: '{}',
      }),
    });
    assert.equal(llmRes.status, 200);

    await waitFor(() => frames.some((f) => f.includes('llm-token')), { timeoutMs: 5000, label: 'llm-token frame' });
    const tokenFrame = frames.find((f) => f.includes('llm-token'));
    const dataLine = tokenFrame.split('\n').find((l) => l.startsWith('data:'));
    const env = JSON.parse(dataLine.slice(5));

    assertWireObject(env, golden().llm_token_event, 'llm_token_event (PARITY#4)', ['type', 'correlation_id', 'session_id', 'client_id', 'payload']);
    assert.equal(env.type, 'llm-token');
    assert.equal(env.payload.chunk, '{"action":"final","content":"parity-stream-final"}');
    assert.equal(env.session_id, 's-parity-stream');
    assert.equal(env.client_id, 'parity-stream-c1');
    try { await reader.cancel(); } catch { }
    sseReader = null;

    /**
     * (b) runTurn streaming: client mengirim ?stream=true sendiri, token ter-feed ke runner,
     * commit deterministik.
     */
    const probe = installFetchProbe();
    runtime = await createAgentRuntime({ serverUrl: url, clientId: 'parity-stream-c1', maxSteps: 5 });
    trackRuntime(runtime);
    const tokenEvents = [];
    const fallbackEvents = [];
    runtime.onEvent((e) => { if (e.type === 'llm-token') tokenEvents.push(e); });
    runtime.onEvent((e) => { if (e.type === 'stream-fallback') fallbackEvents.push(e); });
    await runtime.connect();
    const result = await runtime.runTurn('stream me');

    assert.equal(result.content, 'parity-stream-final');

    const llmPost = probe.calls.find((c) => c.method === 'POST' && c.url.includes(WIRE.LLM_CALL));
    assert.ok(llmPost, 'POST /llm/call must be observed');
    assert.ok(
      new URL(llmPost.url).searchParams.get('stream') === 'true',
      `streaming must be signaled by ?stream=true, got URL: ${llmPost.url}`,
    );
    assert.equal(JSON.parse(llmPost.body.metadata_json ?? '{}').stream, undefined, 'stream flag must not live in metadata_json');

    assert.equal(fallbackEvents.length, 0, 'no stream-fallback expected on a live stream');
    await waitFor(() => tokenEvents.length > 0, { timeoutMs: 5000, label: 'llm-token runtime event' });
    assert.equal(tokenEvents[0].chunk, '{"action":"final","content":"parity-stream-final"}');
  } finally {
    if (sseReader) { try { await sseReader.cancel(); } catch { } }
    if (runtime) { try { runtime.close(); } catch {} untrackRuntime(runtime); }
    clearRuntimeHooksProvider();
    await terminateProc(proc);
    activeSpawns.delete(proc);
  }
});

/**
 * PARITY#5 — Permission (policy client default-deny).
 */

test('PARITY#5 permission: tool client yang tidak di-allow -> denial permission:', async () => {
  const { proc, url } = await spawnPythonServer({
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
    trackRuntime(runtime);
    await runtime.connect();

    await assert.rejects(
      () => runtime.runTurn('use the secret tool'),
      (err) => { assertPermissionError(err, 'runTurn gate denial'); return true; },
    );

    await assert.rejects(
      () => runtime.executeTool('secret_tool', {}),
      (err) => { assertPermissionError(err, 'executeTool gate denial'); return true; },
    );
  } finally {
    if (runtime) { try { runtime.close(); } catch {} untrackRuntime(runtime); }
    clearRuntimeHooksProvider();
    await terminateProc(proc);
    activeSpawns.delete(proc);
  }
});

/**
 * PARITY#6 — Wire-shape consistency (invarian 5).
 */

test('PARITY#6 wire-shape consistency: sample request/response nyata terhadap server Python subset dari golden', async () => {
  const { proc, url } = await spawnPythonServer({
    stubResponse: STUB_FINAL,
    serverTools: ['parity_wire:{"out":1}'],
  });
  const probe = installFetchProbe();
  let runtime = null;
  try {
    runtime = await createAgentRuntime({
      serverUrl: url,
      tools: [makeToolEntry('cli_wire', 'wire client tool', async () => ({ local: true }))],
      policy: { allow: ['cli_wire'] },
      maxSteps: 5,
    });
    trackRuntime(runtime);
    await runtime.connect();
    await runtime.runTurn('wire me');

    const llmPosts = probe.calls.filter((c) => c.method === 'POST' && c.url.includes(WIRE.LLM_CALL));
    assert.ok(llmPosts.length >= 1, 'expected POST /llm/call');
    assertWireObject(llmPosts[0].body, golden().llm_call_request, 'llm_call_request (PARITY#6)', [
      'provider', 'model', 'session_id', 'messages_json', 'force_json', 'temperature', 'max_tokens', 'schema_name', 'metadata_json',
    ]);

    /**
     * Llm-response yang benar-benar dikirim server Python.
     */
    const llmRes = await fetch(`${url}${WIRE.LLM_CALL}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(llmPosts[0].body),
    });
    assert.equal(llmRes.status, 200);
    const llmBody = await llmRes.json();
    assertWireObject(llmBody, golden().llm_call_response, 'llm_call_response (PARITY#6)', [
      'content', 'model', 'session_id', 'message_json', 'tokens_used', 'finish_reason', 'raw_response_json',
    ]);

    /**
     * Tools_list_response: definisi tool server subset golden.
     */
    await runtime.refreshTools();
    const defs = await (await fetch(`${url}${WIRE.TOOLS_LIST}`)).json();
    assert.ok(Array.isArray(defs), 'GET /tools must return an array');
    const goldenDef = golden().tools_list_response[0];
    for (const def of defs) {
      assertNoExtraKeys(def, goldenDef, `tool definition '${def.name}'`);
    }
  } finally {
    if (runtime) { try { runtime.close(); } catch {} untrackRuntime(runtime); }
    clearRuntimeHooksProvider();
    await terminateProc(proc);
    activeSpawns.delete(proc);
  }
});

/**
 * PARITY#7 — Hook decision client (in-process runtime-hooks).
 */

test('PARITY#7 hook decision client: provider override decideAction -> content override dipakai melalui server Python', async () => {
  const { proc, url } = await spawnPythonServer({ stubResponse: STUB_FINAL });
  let runtime = null;
  try {
    let hookCalls = 0;
    runtime = await createAgentRuntime({
      serverUrl: url,
      hooks: {
        decideAction: (stateJson, llmJson) => {
          hookCalls += 1;
          assert.equal(typeof stateJson, 'string');
          assert.equal(typeof llmJson, 'string');
          return { content: 'parity-hook-override' };
        },
      },
      maxSteps: 5,
    });
    trackRuntime(runtime);
    await runtime.connect();
    const result = await runtime.runTurn('override me');
    assert.equal(result.action, 'final');
    assert.equal(result.content, 'parity-hook-override', 'hook decision must replace stub content');
    assert.ok(hookCalls >= 1, 'decideAction must be invoked in-process');
  } finally {
    if (runtime) { try { runtime.close(); } catch {} untrackRuntime(runtime); }
    clearRuntimeHooksProvider();
    await terminateProc(proc);
    activeSpawns.delete(proc);
  }
});
