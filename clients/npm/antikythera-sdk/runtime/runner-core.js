'use strict';

const {
  WIRE,
  joinUrl,
  randomId,
  buildLlmRequest,
  buildToolCallEvent,
  parseLlmResponse,
  parseToolExecutionResult,
  wireToRunnerToolResult,
  normalizeLocalTools,
  runToolHandler,
} = require('./types.js');
const { createTransport } = require('./transport.js');
const { createSseChannel } = require('./sse.js');
const { createControlHandler, acquireRuntimeHooksProvider, releaseRuntimeHooksProvider } = require('./control.js');
const { createPolicyGate } = require('./policy.js');
const { createUnionRegistry } = require('./registry.js');
const { isTool, extractToolEntry } = require('./tools.js');

/**
 * Client core (K1): the WASM runner lives in this module instance. The host
 * runtime owns the tool loop — init, prepare, LLM via the server proxy,
 * commit, drain, route, process — and `auto_execute_tools=false` keeps the
 * runner from executing tools itself.
 */

let bundledRunnerPromise = null;
const remoteRunnerPromises = new Map();

/**
 * Validate a runner namespace at the entry point (injected or imported). The
 * runtime calls `init` first, so a missing init must fail here with a clear
 * message, not mid-session with a TypeError.
 */
function assertRunnerNamespace(runnerNamespace, source) {
  if (!runnerNamespace || typeof runnerNamespace.init !== 'function') {
    throw new Error(`loadRunnerModule: ${source} runner namespace must expose an init function`);
  }
  return runnerNamespace;
}

/** Load the bundled jco component once per module instance (WASM singleton). */
function loadBundledRunner() {
  if (!bundledRunnerPromise) {
    bundledRunnerPromise = import('../component/antikythera-sdk.js').then((module) =>
      assertRunnerNamespace(module.runner, 'component'),
    );
  }
  return bundledRunnerPromise;
}

/** componentBase is an absolute URL directory; exactly one slash precedes the entry file name. */
function componentEntryUrl(componentBase, entry) {
  return componentBase.replace(/\/?$/, '/') + entry;
}

/**
 * Fetch the jco bundle manifest (WIRE_PROTOCOL §2.6). Called only when the
 * consumer opted into `componentBase`; any failure here is an explicit error,
 * never a silent fallback (fallback applies only to the no-option default).
 */
async function fetchComponentManifest(serverUrl) {
  if (typeof serverUrl !== 'string' || !serverUrl) {
    throw new Error('loadRunnerModule: componentBase requires serverUrl to resolve the manifest');
  }
  const url = joinUrl(serverUrl, WIRE.COMPONENT_MANIFEST);
  let response;
  try {
    response = await fetch(url);
  } catch (err) {
    throw new Error(
      `loadRunnerModule: fetch component manifest ${WIRE.COMPONENT_MANIFEST} failed: ${err instanceof Error ? err.message : String(err)}`,
    );
  }
  if (!response.ok) {
    throw new Error(`loadRunnerModule: component manifest ${WIRE.COMPONENT_MANIFEST} failed (HTTP ${response.status})`);
  }
  let manifest;
  try {
    manifest = await response.json();
  } catch (err) {
    throw new Error(
      `loadRunnerModule: component manifest ${WIRE.COMPONENT_MANIFEST} is not valid JSON: ${err instanceof Error ? err.message : String(err)}`,
    );
  }
  if (typeof manifest.entry !== 'string' || !manifest.entry) {
    throw new Error('loadRunnerModule: component manifest is missing a non-empty entry field');
  }
  return manifest;
}

/**
 * Import a component bundle from a resolved URL, cached per URL so two
 * runtimes sharing one bundle instantiate it once (same singleton semantics
 * as the bundled path). An import failure is explicit and mentions the URL.
 */
function loadRemoteRunner(componentBase, entry) {
  const url = componentEntryUrl(componentBase, entry);
  if (!remoteRunnerPromises.has(url)) {
    remoteRunnerPromises.set(
      url,
      import(url).then(
        (module) => assertRunnerNamespace(module.runner, 'component'),
        (err) => {
          throw new Error(
            `loadRunnerModule: import component bundle '${url}' failed: ${err instanceof Error ? err.message : String(err)}`,
          );
        },
      ),
    );
  }
  return remoteRunnerPromises.get(url);
}

/**
 * Load the runner namespace. Priority (D5): injected `runner` > `componentBase`
 * (entry resolved from the server manifest) > bundled path (default, backward
 * compatible — the no-option path makes zero network requests).
 * @param {object} [options]
 * @param {object} [options.runner] - directly injected runner namespace;
 *   bypasses the component import
 * @param {string} [options.componentBase] - absolute URL of the bundle
 *   directory; the entry file is resolved from the server manifest
 * @param {string} [options.serverUrl] - server base URL used for the manifest
 *   fetch; required when `componentBase` is set
 * @returns {Promise<object>}
 */
async function loadRunnerModule(options = {}) {
  if (options.runner != null) {
    return assertRunnerNamespace(options.runner, 'injected');
  }
  if (options.componentBase != null) {
    if (typeof options.componentBase !== 'string' || !options.componentBase) {
      throw new Error('loadRunnerModule: componentBase must be a non-empty absolute URL string');
    }
    const manifest = await fetchComponentManifest(options.serverUrl);
    return loadRemoteRunner(options.componentBase, manifest.entry);
  }
  return loadBundledRunner();
}

/**
 * Streaming commit is deterministic, not racy: the server queues every token
 * on the SSE control channel BEFORE it resolves the POST /llm/call?stream=true
 * response, so the response itself is the stream-completion signal. The token
 * pump drains asynchronously, so we wait for the channel to settle (no new
 * token for SETTLE_MS) before committing the stream; a stream that never
 * produces a token within DRAIN_TIMEOUT_MS falls back explicitly to the
 * response body (never silently to the non-streaming path mid-stream).
 */
const STREAM_SETTLE_MS = 100;
const STREAM_DRAIN_TIMEOUT_MS = 250;

/**
 * @param {object} options
 * @param {string} options.serverUrl
 * @param {Array<{definition: object, handler: Function}>} [options.tools]
 * @param {object|null} [options.hooks]
 * @param {{allow?: Array<string>}} [options.policy]
 * @param {object} [options.llm]
 * @param {number} [options.maxSteps]
 * @param {string} [options.systemPrompt]
 * @param {string} [options.sessionId]
 * @param {string} [options.clientId]
 * @param {string} [options.continuationPrompt]
 * @param {number} [options.sessionTimeoutSecs]
 * @param {number} [options.maxInMemorySessions]
 * @param {object} [options.contextPolicy]
 * @param {string} [options.componentBase] - absolute URL of the jco bundle
 *   directory; the entry file is resolved from the server manifest
 *   (WIRE_PROTOCOL §2.6, decision D5). Omit to keep the bundled component.
 * @param {object} [options.runner] - directly injected runner namespace;
 *   bypasses the component import entirely (decision D5).
 * @returns {Promise<object>}
 */
function normalizeLocalToolsWithSugar(tools) {
  if (!Array.isArray(tools) || tools.length === 0) return normalizeLocalTools(tools);
  const mapped = tools.map((entry) => {
    if (isTool(entry)) {
      const extracted = extractToolEntry(entry);
      return extracted ? { definition: extracted.definition, handler: extracted.handler } : entry;
    }
    return entry;
  });
  return normalizeLocalTools(mapped);
}

async function createClientCoreRuntime(options) {
  if (typeof options.serverUrl !== 'string' || !options.serverUrl) {
    throw new Error('createAgentRuntime: serverUrl is required');
  }
  const transport = createTransport({ serverUrl: options.serverUrl });
  const clientId = options.clientId ?? randomId('client');
  const localEntries = normalizeLocalToolsWithSugar(options.tools);
  const gate = createPolicyGate(options.policy);
  const maxSteps = options.maxSteps ?? 10;
  const continuationPrompt = options.continuationPrompt ?? '[continue]';

  // Hooks provider ownership (R3): the token is held for the runtime's whole
  // lifetime so a coexisting runtime's provider survives this one's teardown.
  const hooksOwnerToken = acquireRuntimeHooksProvider(options.hooks ?? null);
  const runner = await loadRunnerModule({
    serverUrl: options.serverUrl,
    componentBase: options.componentBase,
    runner: options.runner,
  });

  const listeners = new Set();
  let sessionId = options.sessionId ?? null;
  let connected = false;
  let channel = null;
  let registry = null;
  let pendingChunkCount = 0;
  let turnSeq = 0;

  function emitEvent(event) {
    for (const listener of [...listeners]) {
      try {
        listener(event);
      } catch {
        // a UI listener must not break the runtime loop
      }
    }
  }

  function requireConnected() {
    if (!connected || !sessionId) {
      throw new Error('runtime is not connected; call connect() first');
    }
  }

  function onLlmToken(payload) {
    const chunk = payload.chunk;
    if (typeof chunk !== 'string') return;
    if (payload.session_id != null && payload.session_id !== sessionId) {
      emitEvent({ type: 'llm-token', sessionId: payload.session_id, chunk, correlationId: payload.correlation_id ?? null });
      return;
    }
    pendingChunkCount += 1;
    runner.appendLlmChunk(sessionId, chunk, payload.correlation_id ?? null);
    emitEvent({ type: 'llm-token', sessionId, chunk, correlationId: payload.correlation_id ?? null });
  }

  /**
   * Wait until the llm-token stream has settled after the POST /llm/call
   * response resolved. The response is the completion signal; this only
   * absorbs the server's asynchronous token pump. Bounded: never waits longer
   * than STREAM_DRAIN_TIMEOUT_MS.
   */
  async function waitForStreamSettle() {
    const deadline = Date.now() + STREAM_DRAIN_TIMEOUT_MS;
    let quietSince = Date.now();
    for (;;) {
      const now = Date.now();
      if (now - quietSince >= STREAM_SETTLE_MS) return;
      if (now >= deadline) return;
      if (pendingChunkCount > 0) quietSince = now;
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  }

  /**
   * Deterministic stream commit: drain all llm-token events until the stream
   * settles, then commit the streamed chunks. If the stream never produced a
   * token within the bounded drain window, fall back to the llm-response body
   * with an explicit `stream-fallback` event (a dead stream must not silently
   * take the non-streaming path, and must not hang either).
   * @param {string} preparedJson
   * @param {object} llmResponse - parsed llm-response wire body
   * @returns {Promise<object>} parsed runner commit envelope
   */
  async function commitStreamedTurn(preparedJson, llmResponse) {
    await waitForStreamSettle();
    if (pendingChunkCount > 0) {
      return JSON.parse(runner.commitLlmStream(preparedJson));
    }
    emitEvent({
      type: 'stream-fallback',
      reason: 'no llm-token events observed before the stream settle window; committing the llm-response body',
      content: llmResponse.content ?? null,
    });
    const payload = llmResponse.message_json ?? llmResponse.content ?? '';
    return JSON.parse(runner.commitLlmResponse(preparedJson, payload));
  }

  async function refreshRegistry(serverDefinitions) {
    const definitions = serverDefinitions ?? (await transport.pullTools());
    registry = createUnionRegistry({ localEntries, serverDefinitions: definitions });
    const count = runner.registerTools(JSON.stringify(registry.toDefinitions()));
    emitEvent({ type: 'registry', count, owners: registryOwners() });
    return registry;
  }

  function registryOwners() {
    const owners = {};
    for (const entry of localEntries) {
      owners[entry.definition.name] = WIRE.OWNER_CLIENT;
    }
    return owners;
  }

  const control = createControlHandler({
    transport,
    localEntries,
    hooks: options.hooks ?? null,
    gate,
    emit: emitEvent,
    onLlmToken,
    getSessionId: () => sessionId,
    onRegistrySync: (definitions) => {
      try {
        refreshRegistry(definitions).catch((err) => {
          emitEvent({ type: 'error', error: err instanceof Error ? err.message : String(err) });
        });
      } catch (err) {
        emitEvent({ type: 'error', error: err instanceof Error ? err.message : String(err) });
      }
    },
  });

  function eventsUrl() {
    const query = `client_id=${encodeURIComponent(clientId)}`;
    const sessionQuery = sessionId ? `&session_id=${encodeURIComponent(sessionId)}` : '';
    return joinUrl(options.serverUrl, `${WIRE.EVENTS}?${query}${sessionQuery}`);
  }

  async function connect() {
    if (connected) return;
    const config = {
      session_id: options.sessionId ?? undefined,
      max_steps: maxSteps,
      auto_execute_tools: false,
      runtime_hooks_enabled: true,
      session_timeout_secs: options.sessionTimeoutSecs ?? undefined,
      max_in_memory_sessions: options.maxInMemorySessions ?? undefined,
      context_policy: options.contextPolicy ?? undefined,
    };
    sessionId = runner.init(JSON.stringify(config));
    emitEvent({ type: 'session', sessionId });
    await refreshRegistry();
    channel = createSseChannel({
      url: eventsUrl(),
      onEvent: (envelope) => control.handle(envelope),
      onStatus: (status) => emitEvent({ type: 'status', ...status }),
    });
    channel.start();
    connected = true;
  }

  async function routeTool(toolName, args, stepId, correlationId) {
    const owner = registry ? registry.ownerOf(toolName) : undefined;
    if (owner === WIRE.OWNER_CLIENT) {
      const result = await runLocalTool(toolName, args);
      return {
        tool_name: toolName,
        success: result.success,
        output_json: result.output_json,
        error_message: result.error_message,
        correlation_id: correlationId ?? null,
      };
    }
    if (owner === WIRE.OWNER_SERVER || owner === WIRE.OWNER_MCP) {
      const wireCall = buildToolCallEvent({
        toolName,
        argumentsJson: JSON.stringify(args ?? {}),
        sessionId,
        stepId,
      });
      const wireResult = parseToolExecutionResult(await transport.executeServerTool(wireCall));
      return wireToRunnerToolResult(wireResult, correlationId);
    }
    throw new Error(`permission: tool '${toolName}' has no owner in the union registry`);
  }

  async function runLocalTool(toolName, args) {
    gate.check(toolName);
    const entry = localEntries.find((item) => item.definition.name === toolName);
    if (!entry) {
      throw new Error(`permission: tool '${toolName}' is not owned by this client`);
    }
    return runToolHandler(entry.handler, args);
  }

  async function executeTool(toolName, args = {}) {
    requireConnected();
    return routeTool(toolName, args, 0, null);
  }

  /**
   * Auto tool-owner loop (K1): prepare -> LLM via server proxy (SSE tokens
   * feed append-llm-chunk) -> commit stream -> drain -> route tool result ->
   * process -> repeat until final / max_steps / retry.
   * @param {string} prompt
   * @param {object} [opts]
   * @param {string} [opts.systemPrompt]
   * @param {boolean} [opts.forceJson]
   * @param {string} [opts.metadataJson]
   * @param {string} [opts.correlationId]
   * @param {string} [opts.continuationPrompt]
   * @returns {Promise<{sessionId: string, action: 'final', content: string|null, events: Array<object>, iterations: number}>}
   */
  async function runTurn(prompt, opts = {}) {
    requireConnected();
    const systemPrompt = opts.systemPrompt ?? options.systemPrompt ?? '';
    const turnCorrelationId = opts.correlationId ?? `turn-${++turnSeq}`;
    const continuation = opts.continuationPrompt ?? continuationPrompt;
    const maxIterations = maxSteps + 1;
    const allEvents = [];
    let iteration = 0;
    let first = true;

    while (true) {
      iteration += 1;
      if (iteration > maxIterations) {
        throw new Error(`permission: turn exceeded max_steps (${maxSteps}) iterations`);
      }

      const correlationId = `${turnCorrelationId}-${iteration}`;
      const preparedJson = runner.prepareUserTurn(
        JSON.stringify({
          prompt: first ? prompt : continuation,
          session_id: sessionId,
          system_prompt: systemPrompt,
          force_json: opts.forceJson ?? false,
          metadata_json: opts.metadataJson ?? null,
          correlation_id: correlationId,
        }),
      );
      const prepared = JSON.parse(preparedJson);

      pendingChunkCount = 0;
      const streamRequested = options.llm?.stream !== false;
      const llmRequest = buildLlmRequest({
        provider: options.llm?.provider ?? null,
        model: options.llm?.model ?? null,
        sessionId,
        messagesJson: prepared.messages_json,
        forceJson: prepared.force_json,
        temperature: options.llm?.temperature ?? null,
        maxTokens: options.llm?.maxTokens ?? null,
        schemaName: options.llm?.schemaName ?? null,
        metadataJson: opts.metadataJson ?? options.llm?.metadataJson ?? null,
      });
      const llmResponse = parseLlmResponse(
        await transport.llmCall(llmRequest, { stream: streamRequested }),
      );

      let commit;
      if (streamRequested) {
        commit = await commitStreamedTurn(preparedJson, llmResponse);
      } else {
        const payload = llmResponse.message_json ?? llmResponse.content ?? '';
        commit = JSON.parse(runner.commitLlmResponse(preparedJson, payload));
      }

      const drained = JSON.parse(runner.drainEvents(sessionId));
      allEvents.push(...drained);
      for (const event of drained) {
        emitEvent({ type: 'runner-event', ...event });
      }

      if (commit.action === 'final') {
        emitEvent({ type: 'final', sessionId, content: commit.content ?? null });
        return {
          sessionId,
          action: 'final',
          content: commit.content ?? null,
          events: allEvents,
          iterations: iteration,
        };
      }

      if (commit.action === 'call_tool') {
        const toolName = commit.tool_name;
        const input = commit.tool_input ?? {};
        emitEvent({ type: 'tool_requested', sessionId, tool: toolName, input, step: commit.step ?? 0 });
        const toolResultInput = await routeTool(toolName, input, commit.step ?? 0, correlationId);
        emitEvent({ type: 'tool_result', sessionId, tool: toolName, success: toolResultInput.success });
        runner.processToolResultForSession(sessionId, JSON.stringify(toolResultInput));
        first = false;
        continue;
      }

      if (commit.action === 'retry') {
        throw new Error(`LLM retry requested: ${commit.content ?? 'unknown'}`);
      }

      throw new Error(`unexpected commit action: ${commit.action}`);
    }
  }

  function getState() {
    requireConnected();
    return JSON.parse(runner.getState(sessionId));
  }

  function getToolsPrompt() {
    return runner.getToolsPrompt();
  }

  function resetSession() {
    if (!sessionId) return false;
    const removed = runner.resetSession(sessionId);
    if (channel) {
      channel.stop();
      channel = null;
    }
    sessionId = null;
    connected = false;
    return removed;
  }

  function close() {
    if (channel) {
      channel.stop();
      channel = null;
    }
    connected = false;
    releaseRuntimeHooksProvider(hooksOwnerToken);
  }

  const runtime = {
    core: 'client',
    serverUrl: options.serverUrl,
    clientId,
    get sessionId() {
      return sessionId;
    },
    get connected() {
      return connected;
    },
    connect,
    close,
    onEvent(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    runTurn,
    executeTool,
    getState,
    getToolsPrompt,
    resetSession,
    refreshTools: () => refreshRegistry(),
  };

  return runtime;
}

module.exports = { createClientCoreRuntime, loadRunnerModule };
