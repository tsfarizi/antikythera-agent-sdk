'use strict';

const {
  WIRE,
  joinUrl,
  randomId,
  normalizeLocalTools,
  runToolHandler,
} = require('./types.js');
const { createTransport } = require('./transport.js');
const { createSseChannel } = require('./sse.js');
const { createControlHandler } = require('./control.js');
const { createPolicyGate } = require('./policy.js');
const { isTool, extractToolEntry } = require('./tools.js');

/**
 * Server core: the WASM runner lives on the server, so this module does NOT
 * load the component. The client is a control-channel peer: it executes
 * client-owned tools on `tool-execution-request`, answers `hook-request` with
 * the client hook decision, and forwards progress events (llm-token,
 * event-forward, lifecycle, error) to the UI. The loop and LLM routing are
 * owned by the server.
 */

/**
 * Normalize tool classes and decorated handlers into validated registry entries.
 * @param {Array<object>} [tools] - Client-owned tool definitions and handlers.
 * @returns {Array<object>} Entries accepted by the local tool registry.
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
 * Create a control-channel peer for a runner owned by the server.
 * Client tools and hooks use this peer's registry and permission gate.
 * @param {object} options
 * @param {string} options.serverUrl
 * @param {Array<{definition: object, handler: Function}>} [options.tools]
 * @param {object|null} [options.hooks]
 * @param {{allow?: Array<string>}} [options.policy]
 * @param {string} [options.clientId]
 * @param {string} [options.sessionId]
 * @returns {Promise<object>}
 */
async function createServerCoreRuntime(options) {
  if (typeof options.serverUrl !== 'string' || !options.serverUrl) {
    throw new Error('createAgentRuntime: serverUrl is required');
  }
  const clientId = options.clientId ?? randomId('client');
  const transport = createTransport({ serverUrl: options.serverUrl, clientId, authToken: options.authToken, timeoutMs: options.timeoutMs });
  const sessionId = options.sessionId ?? null;
  const localEntries = normalizeLocalToolsWithSugar(options.tools);
  const gate = createPolicyGate(options.policy);

  const listeners = new Set();
  let connected = false;
  let channel = null;
  let connectionPromise = null;

  /** Deliver a progress event to every subscriber while isolating subscriber failures. */
  function emitEvent(event) {
    for (const listener of [...listeners]) {
      try {
        listener(event);
      } catch {
      }
    }
  }

  async function executeLocalTool(toolName, args = {}) {
    gate.check(toolName);
    const entry = localEntries.find((item) => item.definition.name === toolName);
    if (!entry) {
      throw new Error(`permission: tool '${toolName}' is not owned by this client`);
    }
    const result = await runToolHandler(entry.handler, args);
    return {
      tool_name: toolName,
      success: result.success,
      output_json: result.output_json,
      error_message: result.error_message,
      correlation_id: null,
    };
  }

  const control = createControlHandler({
    transport,
    localEntries,
    hooks: options.hooks ?? null,
    gate,
    emit: emitEvent,
    onLlmToken: (payload) => emitEvent({ type: 'llm-token', sessionId: sessionId ?? payload.session_id ?? null, payload }),
    getSessionId: () => sessionId,
    onRegistrySync: (definitions) => emitEvent({ type: 'registry-sync', definitions }),
  });

  function eventsUrl() {
    const query = `client_id=${encodeURIComponent(clientId)}`;
    const sessionQuery = sessionId ? `&session_id=${encodeURIComponent(sessionId)}` : '';
    return joinUrl(options.serverUrl, `${WIRE.EVENTS}?${query}${sessionQuery}`);
  }

  async function connect() {
    if (connected) return;
    if (connectionPromise) return connectionPromise;
    connectionPromise = (async () => {
      const connectingChannel = createSseChannel({
        url: eventsUrl(),
        timeoutMs: options.timeoutMs,
        headers: options.authToken ? { Authorization: `Bearer ${options.authToken}` } : {},
        onEvent: (envelope) => control.handle(envelope),
        onStatus: (status) => emitEvent({ type: 'status', ...status }),
      });
      channel = connectingChannel;
      try {
        connectingChannel.start();
        await connectingChannel.ready();
        if (channel !== connectingChannel) throw new Error('runtime connection closed');
        connected = true;
      } catch (error) {
        connectingChannel.stop();
        if (channel === connectingChannel) channel = null;
        throw error;
      } finally { connectionPromise = null; }
    })();
    return connectionPromise;
  }

  function close() {
    if (channel) {
      channel.stop();
      channel = null;
    }
    connected = false;
  }

  return {
    core: 'server',
    serverUrl: options.serverUrl,
    clientId,
    sessionId,
    get connected() {
      return connected;
    },
    connect,
    close,
    onEvent(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    executeLocalTool,
  };
}

module.exports = { createServerCoreRuntime };
