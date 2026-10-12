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
const { createControlHandler } = require('./control.js');
const { createPolicyGate } = require('./policy.js');
const { createUnionRegistry } = require('./registry.js');
const { isTool, extractToolEntry } = require('./tools.js');

/**
 * Client core: each runtime owns a WASM instance created by the jco factory. The host
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
async function fetchComponentManifest(serverUrl, options = {}) {
  if (typeof serverUrl !== 'string' || !serverUrl) {
    throw new Error('loadRunnerModule: componentBase requires serverUrl to resolve the manifest');
  }
  const url = joinUrl(serverUrl, WIRE.COMPONENT_MANIFEST);
  let response;
  try {
    response = await fetch(url, {headers: options.authToken ? { Authorization: `Bearer ${options.authToken}` } : {}, signal: AbortSignal.timeout(options.timeoutMs ?? 60000) });
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
    const manifest = await fetchComponentManifest(options.serverUrl, options);
    return loadRemoteRunner(options.componentBase, manifest.entry);
  }
  return loadBundledRunner();
}

/**
 * Normalize tool classes, decorated handlers, and explicit definition/handler entries
 * into the local registry representation.
 * @param {Array<object>} [tools] - Tool entries supplied by the application.
 * @returns {Array<object>} Validated definition/handler entries.
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

/**
 * Create a host tool loop with a private WASM Component Model instance.
 *
 * Hooks, sessions, cancellation, and tool registries belong to the returned runtime.
 * Remote components load their instantiate.js factory after validating the manifest.
 * An injected runner bypasses factory creation and retains its caller-defined ownership.
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
 * @returns {Promise<object>} Runtime lifecycle, turn, event, and registry operations.
 */
async function createClientCoreRuntime(options) {
  if (typeof options.serverUrl !== 'string' || !options.serverUrl) {
    throw new Error('createAgentRuntime: serverUrl is required');
  }
  const clientId = options.clientId ?? randomId('client');
  const transport = createTransport({ serverUrl: options.serverUrl, authToken: options.authToken, timeoutMs: options.timeoutMs, clientId });
  const localEntries = normalizeLocalToolsWithSugar(options.tools);
  const gate = createPolicyGate(options.policy);
  const maxSteps = options.maxSteps ?? 10;
  if (!Number.isInteger(maxSteps) || maxSteps < 0) throw new Error('maxSteps must be a non-negative integer');
  const continuationPrompt = options.continuationPrompt ?? '[continue]';

  let runner;
  if (options.runner != null) {
    runner = assertRunnerNamespace(options.runner, 'injected');
  } else {
    let entry = '../component/instantiate.js';
    if (options.componentBase != null) {
      await fetchComponentManifest(options.serverUrl, options);
      entry = componentEntryUrl(options.componentBase, 'instantiate.js');
    }
    try {
      runner = assertRunnerNamespace(await (await import(entry)).createRunner(options.hooks), 'component');
    } catch (err) {
      throw new Error(`loadRunnerModule: import component bundle '${entry}' failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  const listeners = new Set();
  let sessionId = options.sessionId ?? null;
  let connected = false;
  let channel = null;
  let registry = null;
  let turnSeq = 0;
  let activeTurn = null;
  let resetPending = false;
  let closed = false;

  /** Deliver a runtime event to every subscriber while isolating subscriber failures. */
  function emitEvent(event) {
    for (const listener of [...listeners]) {
      try {
        listener(event);
      } catch {
      }
    }
  }

  function requireConnected() {
    if (!connected || !sessionId) {
      throw new Error('runtime is not connected; call connect() first');
    }
  }

  /**
   * Forward SSE tokens for presentation without committing them to WASM state.
   * The complete HTTP response remains authoritative when frames are late,
   * duplicated, or lost. Tokens for other sessions retain their original identity.
   */
  function onLlmToken(payload) {
    const chunk = payload.chunk;
    if (typeof chunk !== 'string') return;
    if (payload.session_id != null && payload.session_id !== sessionId) {
      emitEvent({ type: 'llm-token', sessionId: payload.session_id, chunk, correlationId: payload.correlation_id ?? null });
      return;
    }
    emitEvent({ type: 'llm-token', sessionId, chunk, correlationId: payload.correlation_id ?? null });
  }

  /** Commit the complete HTTP response once, then publish the resulting runner action. */
  async function commitStreamedTurn(preparedJson, llmResponse) {
    emitEvent({ type: 'stream-complete', source: 'http', content: llmResponse.content });
    return JSON.parse(runner.commitLlmResponse(preparedJson, llmResponse.content ?? ''));
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

  let connectionPromise = null;
  async function connect() {
    if (!connectionPromise) connectionPromise = connectOnce().finally(() => { connectionPromise = null; });
    return connectionPromise;
  }
  async function connectOnce() {
    if (closed) throw new Error('runtime is closed');
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
    if (closed) throw new Error('runtime is closed');
    const connectingChannel = createSseChannel({
      url: eventsUrl(),
      timeoutMs: options.timeoutMs,
      headers: options.authToken ? { Authorization: `Bearer ${options.authToken}` } : {},
      onEvent: (envelope) => control.handle(envelope),
      onStatus: (status) => emitEvent({ type: 'status', ...status }),
    });
    channel = connectingChannel;
    connectingChannel.start();
    try { await connectingChannel.ready(); } catch (error) {
      connectingChannel.stop();
      if (channel === connectingChannel) channel = null;
      throw error;
    }
    if (closed) throw new Error('runtime is closed');
    connected = true;
  }

  async function routeTool(toolName, args, stepId, correlationId, signal) {
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
      const wireResult = parseToolExecutionResult(await transport.executeServerTool(wireCall, { signal }));
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
  async function performTurn(prompt, opts = {}) {
    requireConnected();
    const systemPrompt = opts.systemPrompt ?? options.systemPrompt ?? '';
    const turnCorrelationId = opts.correlationId ?? `turn-${++turnSeq}`;
    const continuation = opts.continuationPrompt ?? continuationPrompt;
    const requestedSteps = opts.maxSteps ?? maxSteps;
    if (!Number.isInteger(requestedSteps) || requestedSteps < 0) {
      throw new Error('maxSteps must be a non-negative integer');
    }
    const maxIterations = Math.min(maxSteps, requestedSteps);
    const allEvents = [];
    let iteration = 0;
    let first = true;

    while (true) {
      opts.signal?.throwIfAborted();
      if (iteration >= maxIterations) {
        throw new Error(`permission: turn exceeded max_steps (${maxSteps}) iterations`);
      }

      iteration += 1;
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
      if (activeTurn) activeTurn.steps = iteration;
      const llmResponse = parseLlmResponse(
        await transport.llmCall(llmRequest, { stream: streamRequested, signal: opts.signal }),
      );

      opts.signal?.throwIfAborted();
      let commit;
      if (streamRequested) {
        commit = await commitStreamedTurn(preparedJson, llmResponse);
      } else {
        const payload = llmResponse.content ?? '';
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
        if (drained.some(event => event.kind === 'tool_result' && event.payload?.tool === toolName)) { first = false; continue; }
        const input = commit.tool_input ?? {};
        opts.signal?.throwIfAborted();
        emitEvent({ type: 'tool_requested', sessionId, tool: toolName, input, step: commit.step ?? 0 });
        const toolResultInput = await routeTool(toolName, input, commit.step ?? 0, correlationId, opts.signal);
        emitEvent({ type: 'tool_result', sessionId, tool: toolName, success: toolResultInput.success });
        opts.signal?.throwIfAborted();
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

  async function runTurn(prompt, opts = {}) {
    if (activeTurn) throw new Error('runtime already has an active turn');
    const controller = new AbortController();
    const turn = { controller, steps: 0 };
    activeTurn = turn;
    try {
      if (opts.signal) {
        if (opts.signal.aborted) controller.abort(opts.signal.reason);
        else opts.signal.addEventListener('abort', turn.abortParent = () => controller.abort(opts.signal.reason), { once: true });
      }
      return await performTurn(prompt, { ...opts, signal: controller.signal });
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      err.stepsUsed = turn.steps;
      err.sessionId = sessionId;
      throw err;
    } finally {
      if (turn.abortParent) opts.signal.removeEventListener('abort', turn.abortParent);
      activeTurn = null;
      if (resetPending) { resetPending = false; resetSession(); }
    }
  }

  function cancelTurn() {
    if (!activeTurn) return false;
    activeTurn.controller.abort(new Error('runtime turn cancelled'));
    return true;
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
    if (activeTurn) { resetPending = true; cancelTurn(); return true; }
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
    closed = true;
    cancelTurn();
    if (channel) {
      channel.stop();
      channel = null;
    }
    connected = false;

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
    cancelTurn,
    executeTool,
    getState,
    getToolsPrompt,
    resetSession,
    refreshTools: () => refreshRegistry(),
  };

  return runtime;
}

module.exports = { createClientCoreRuntime, loadRunnerModule };
