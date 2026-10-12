'use strict';

/**
 * Antikythera runtime bridge — high-level host runtime for the WASM agent
 * core. See documentation/WIRE_PROTOCOL.md.
 */

const { createClientCoreRuntime, loadRunnerModule } = require('./runner-core.js');
const { createServerCoreRuntime } = require('./peer-core.js');
const { createPolicyGate } = require('./policy.js');
const { createUnionRegistry } = require('./registry.js');
const { createTransport } = require('./transport.js');
const { createSseChannel } = require('./sse.js');
const { createControlHandler, installRuntimeHooksProvider, acquireRuntimeHooksProvider, releaseRuntimeHooksProvider, wrapHookFunction, invokeHook } = require('./control.js');
const wire = require('./types.js');
const { defineTool, isTool, extractToolEntry, registerTool, DEFAULT_INPUT_SCHEMA, TOOL_SYMBOL } = require('./tools.js');

/**
 * Create a host runtime for the Antikythera agent core.
 *
 * @param {object} options
 * @param {'client'|'server'} [options.core] - where the WASM runner lives
 *   (default 'client'). 'client' loads the component and runs the tool-owner
 *   loop; 'server' is a control-channel peer without the component.
 * @param {string} options.serverUrl - base URL of the Antikythera server
 * @param {Array<{definition: object, handler: Function}>} [options.tools] -
 *   client-owned tool definitions + handlers (locked to the client)
 * @param {{prepareTurn?: Function, decideAction?: Function, handleToolResult?: Function}} [options.hooks] -
 *   runtime hooks provider (injected via
 *   `globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__`)
 * @param {{allow?: Array<string>}} [options.policy] - client-side permission
 *   policy; default-deny allowlist for local tool execution
 * @param {object} [options.llm] - LLM proxy options sent with every
 *   `/llm/call` ({provider, model, temperature, maxTokens, schemaName, stream})
 * @param {number} [options.maxSteps] - runner max_steps (default 10)
 * @param {string} [options.systemPrompt] - default system prompt for turns
 * @param {string} [options.continuationPrompt] - prompt used for loop
 *   iterations after a tool result (default '[continue]')
 * @param {string} [options.componentBase] - absolute URL of the jco bundle
 *   directory; the entry file is resolved from the server manifest
 *   (WIRE_PROTOCOL §2.6, decision D5). Client core only; omit to keep the
 *   bundled component (default).
 * @param {object} [options.runner] - directly injected runner namespace;
 *   bypasses the component import entirely (decision D5). Client core only.
 * @returns {Promise<object>} the runtime instance
 */
async function createAgentRuntime(options = {}) {
  const core = options.core ?? 'client';
  if (core === 'server') {
    return createServerCoreRuntime(options);
  }
  if (core === 'client') {
    return createClientCoreRuntime(options);
  }
  throw new Error(`createAgentRuntime: unknown core '${core}' (expected 'client' or 'server')`);
}

module.exports = {
  createAgentRuntime,
  createClientCoreRuntime,
  createServerCoreRuntime,
  createPolicyGate,
  createUnionRegistry,
  createTransport,
  createSseChannel,
  createControlHandler,
  installRuntimeHooksProvider,
  acquireRuntimeHooksProvider,
  releaseRuntimeHooksProvider,
  wrapHookFunction,
  invokeHook,
  loadRunnerModule,
  defineTool,
  isTool,
  extractToolEntry,
  registerTool,
  DEFAULT_INPUT_SCHEMA,
  TOOL_SYMBOL,
  ...wire,
};
