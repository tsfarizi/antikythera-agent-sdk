'use strict';

/**
 * Shared vocabulary for the runtime bridge: wire-shape builders/parsers,
 * runner mapping helpers, and small utilities. One concern: the exact byte
 * shapes defined by WIRE_PROTOCOL.md and contracts/shared/wire_protocol.golden.json.
 *
 * Key conventions (from the golden file):
 * - llm-request / llm-response use snake_case field names.
 * - tool-call-event / tool-execution-result use kebab-case field names.
 * - The SSE event envelope and the POST-back use snake_case.
 * - The runner ToolResultInput uses snake_case; wire step-id is dropped.
 */

const WIRE = {
  LLM_CALL: '/antikythera/v1/llm/call',
  TOOLS_EXECUTE: '/antikythera/v1/tools/execute',
  TOOLS_LIST: '/antikythera/v1/tools',
  EVENTS: '/antikythera/v1/events',
  COMPONENT_MANIFEST: '/antikythera/v1/component/manifest',
  OWNER_CLIENT: 'client',
  OWNER_SERVER: 'server',
  OWNER_MCP: 'mcp',
  HOOK_PREPARE_TURN: 'prepare-turn',
  HOOK_DECIDE_ACTION: 'decide-action',
  HOOK_HANDLE_TOOL_RESULT: 'handle-tool-result',
  PASSTHROUGH: '{"passthrough": true}',
};

/**
 * Join a base URL and a path, tolerating a trailing slash on the base.
 * @param {string} serverUrl
 * @param {string} path
 * @returns {string}
 */
function joinUrl(serverUrl, path) {
  return serverUrl.replace(/\/+$/, '') + path;
}

/**
 * Opaque id generator for client_id and correlation defaults.
 * @param {string} prefix
 * @returns {string}
 */
function randomId(prefix) {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

/**
 * Build the wire `llm-request` body (snake_case, golden `llm_call_request`).
 * Streaming is signaled by the `?stream=true` query parameter on the endpoint
 * (WIRE_PROTOCOL §2.1), NOT inside the body; `metadata_json` is reserved for
 * provider metadata only.
 * @param {object} input
 * @param {string|null} [input.provider]
 * @param {string|null} [input.model]
 * @param {string|null} [input.sessionId]
 * @param {string} [input.messagesJson]
 * @param {boolean} [input.forceJson]
 * @param {number|null} [input.temperature]
 * @param {number|null} [input.maxTokens]
 * @param {string|null} [input.schemaName]
 * @param {string|null} [input.metadataJson]
 * @returns {object}
 */
function buildLlmRequest(input) {
  return {
    provider: input.provider ?? null,
    model: input.model ?? null,
    session_id: input.sessionId ?? null,
    messages_json: input.messagesJson ?? '',
    force_json: input.forceJson ?? false,
    temperature: input.temperature ?? null,
    max_tokens: input.maxTokens ?? null,
    schema_name: input.schemaName ?? null,
    metadata_json: input.metadataJson ?? null,
  };
}

/**
 * Validate/parse a wire `llm-response` body (golden `llm_call_response`).
 * @param {object} body
 * @returns {{content: string, model: string|null, session_id: string|null, message_json: string|null, tokens_used: number|null, finish_reason: string|null, raw_response_json: string|null}}
 */
function parseLlmResponse(body) {
  if (!body || typeof body !== 'object') {
    throw new Error('llm/call response is not an object');
  }
  return {
    content: typeof body.content === 'string' ? body.content : '',
    model: body.model ?? null,
    session_id: body.session_id ?? null,
    message_json: body.message_json ?? null,
    tokens_used: body.tokens_used ?? null,
    finish_reason: body.finish_reason ?? null,
    raw_response_json: body.raw_response_json ?? null,
  };
}

/**
 * Build the wire `tool-call-event` body (kebab-case, golden
 * `tool_execute_request`).
 * @param {object} input
 * @param {string} input.toolName
 * @param {string} input.argumentsJson
 * @param {string|null} [input.sessionId]
 * @param {number} [input.stepId]
 * @returns {object}
 */
function buildToolCallEvent(input) {
  return {
    'tool-name': input.toolName,
    'arguments-json': input.argumentsJson ?? '{}',
    'session-id': input.sessionId ?? null,
    'step-id': input.stepId ?? 0,
  };
}

/**
 * Validate/parse a wire `tool-execution-result` body (kebab-case, golden
 * `tool_execute_response`).
 * @param {object} body
 * @returns {{'tool-name': string, success: boolean, 'output-json': string, 'error-message': string|null, 'step-id': number}}
 */
function parseToolExecutionResult(body) {
  if (!body || typeof body !== 'object') {
    throw new Error('tools/execute response is not an object');
  }
  return {
    'tool-name': body['tool-name'] ?? '',
    success: body.success === true,
    'output-json': body['output-json'] ?? '{}',
    'error-message': body['error-message'] ?? null,
    'step-id': body['step-id'] ?? 0,
  };
}

/**
 * WIRE_PROTOCOL §6 mapping: wire `tool-execution-result` -> runner
 * `ToolResultInput`. `step_id` is dropped (the runner derives it from session
 * state); `output_json` is required; `correlation_id` is forwarded from the
 * pending call when present.
 * @param {object} wireResult - parsed tool-execution-result (kebab keys)
 * @param {string|null} [correlationId]
 * @returns {{tool_name: string, success: boolean, output_json: string, error_message: string|null, correlation_id: string|null}}
 */
function wireToRunnerToolResult(wireResult, correlationId = null) {
  return {
    tool_name: wireResult['tool-name'],
    success: wireResult.success === true,
    output_json: wireResult['output-json'] ?? '{}',
    error_message: wireResult['error-message'] ?? null,
    correlation_id: correlationId ?? null,
  };
}

/**
 * Parse a wire SSE event envelope (golden `*_event` shapes).
 * @param {object} data
 * @returns {{type: string, correlation_id: string|null, session_id: string|null, client_id: string|null, payload: any}}
 */
function parseEventEnvelope(data) {
  if (!data || typeof data !== 'object') {
    throw new Error('SSE event data is not an object');
  }
  return {
    type: data.type,
    correlation_id: data.correlation_id ?? null,
    session_id: data.session_id ?? null,
    client_id: data.client_id ?? null,
    payload: data.payload ?? null,
  };
}

/**
 * Build a POST-back body (golden `postback_response` / `postback_gate_denial`).
 * @param {object} input
 * @param {string} input.correlationId
 * @param {boolean} input.ok
 * @param {*} [input.payload]
 * @param {string|null} [input.error]
 * @returns {object}
 */
function buildPostback(input) {
  return {
    correlation_id: input.correlationId,
    ok: input.ok === true,
    payload: input.payload ?? null,
    error: input.error ?? null,
  };
}

/**
 * Normalize a `tools` option array into `{definition, handler}` entries.
 * Every entry must carry a definition with a name and a callable handler.
 * Also accepts ToolRef sugar (defineTool) via Symbol.for('antikythera.tool').
 * @param {Array} [tools]
 * @returns {Array<{definition: object, handler: Function}>}
 */
function normalizeLocalTools(tools) {
  const entries = Array.isArray(tools) ? tools : [];
  return entries.map((rawEntry, index) => {
    let entry = rawEntry;
    if (entry != null && (typeof entry === 'object' || typeof entry === 'function')) {
      const sym = Symbol.for('antikythera.tool');
      const maybe = entry[sym];
      if (maybe && typeof maybe === 'object' && maybe !== null && typeof maybe.handler === 'function' && maybe.definition && typeof maybe.definition === 'object') {
        entry = { definition: maybe.definition, handler: maybe.handler };
      }
    }
    if (!entry || typeof entry !== 'object') {
      throw new Error(`tool entry #${index}: expected { definition, handler }`);
    }
    const definition = entry.definition;
    const handler = entry.handler;
    if (!definition || typeof definition.name !== 'string' || !definition.name) {
      throw new Error(`tool entry #${index}: definition.name is required`);
    }
    if (typeof handler !== 'function') {
      throw new Error(`tool '${definition.name}': handler is required`);
    }
    return { definition, handler };
  });
}

/**
 * Execute a tool handler and normalize the outcome to the runner
 * `ToolResultInput`-compatible `{success, output_json, error_message}`.
 * A handler may return `{success, output?, error?}` or a plain value (treated
 * as a successful output). A thrown handler is a tool failure, not a gate
 * denial.
 * @param {Function} handler
 * @param {*} args
 * @returns {Promise<{success: boolean, output_json: string, error_message: string|null}>}
 */
async function runToolHandler(handler, args) {
  try {
    const raw = await handler(args);
    if (raw && typeof raw === 'object' && typeof raw.success === 'boolean') {
      if (raw.success) {
        return {
          success: true,
          output_json: JSON.stringify(raw.output ?? null),
          error_message: null,
        };
      }
      return {
        success: false,
        output_json: raw.output !== undefined ? JSON.stringify(raw.output) : '{}',
        error_message: raw.error ?? 'tool failed',
      };
    }
    return { success: true, output_json: JSON.stringify(raw ?? null), error_message: null };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { success: false, output_json: '{}', error_message: message };
  }
}

module.exports = {
  WIRE,
  joinUrl,
  randomId,
  buildLlmRequest,
  parseLlmResponse,
  buildToolCallEvent,
  parseToolExecutionResult,
  wireToRunnerToolResult,
  parseEventEnvelope,
  buildPostback,
  normalizeLocalTools,
  runToolHandler,
};
