'use strict';

const { WIRE, joinUrl } = require('./types.js');

/**
 * HTTP transport for the wire protocol. Every request targets paths under
 * `${serverUrl}/antikythera/v1/*` — there is deliberately no other host
 * constant in this module (invariant: the client never fetches an LLM
 * provider directly; all LLM traffic proxies through the server).
 */

/**
 * @param {{ serverUrl: string }} options
 * @returns {{
 *   llmCall: (llmRequest: object, options?: {stream?: boolean}) => Promise<object>,
 *   executeServerTool: (toolCallEvent: object) => Promise<object>,
 *   pullTools: () => Promise<Array<object>>,
 *   postback: (correlationId: string, postbackBody: object) => Promise<null>,
 * }}
 */
function createTransport({ serverUrl, authToken, timeoutMs = 60000, clientId }) {
  if (typeof serverUrl !== 'string' || !serverUrl) {
    throw new Error('transport: serverUrl is required');
  }

  if (!Number.isInteger(timeoutMs) || timeoutMs < 1) throw new Error('transport: timeoutMs must be a positive integer');
  /**
   * Send a wire request bounded by the transport timeout and optional caller cancellation.
   * Timer and abort listeners are released after both successful and failed requests.
   * @param {string} path - Wire endpoint relative to the configured server URL.
   * @param {{method?: string, body?: object, signal?: AbortSignal}} [opts] - Request options.
   * @returns {Promise<object|null>} Decoded response or null for an empty response.
   */
  async function request(path, opts = {}) {
    const controller = new AbortController();
    const cancel = () => controller.abort(opts.signal?.reason);
    if (opts.signal?.aborted) cancel();
    else opts.signal?.addEventListener('abort', cancel, { once: true });
    const timer = setTimeout(() => controller.abort(new Error('transport request timed out')), timeoutMs);
    try { return await doRequest(path, { ...opts, signal: controller.signal }); }
    finally { clearTimeout(timer); opts.signal?.removeEventListener('abort', cancel); }
  }
  async function doRequest(path, opts = {}) {
    const method = opts.method ?? 'GET';
    const hasBody = opts.body !== undefined;
    let response;
    try {
      response = await fetch(joinUrl(serverUrl, path), {
        method,
        headers: { ...(hasBody ? { 'Content-Type': 'application/json' } : {}), ...(authToken ? { Authorization: `Bearer ${authToken}` } : {}), ...(clientId ? { 'X-Antikythera-Client-Id': clientId } : {}) },
        signal: opts.signal,
        body: hasBody ? JSON.stringify(opts.body) : undefined,
      });
    } catch (err) {
      throw new Error(`transport: ${method} ${path} failed: ${err instanceof Error ? err.message : String(err)}`);
    }
    if (response.status === 204) {
      return null;
    }
    const text = await response.text();
    let json = null;
    if (text) {
      try {
        json = JSON.parse(text);
      } catch {
        json = null;
      }
    }
    if (!response.ok) {
      const message =
        (json && (typeof json.error === 'string' ? json.error : json.message)) ||
        text ||
        `HTTP ${response.status}`;
      throw new Error(String(message));
    }
    return json;
  }

  return {
    /**
     * POST /llm/call — the wire body never carries the stream flag; streaming
     * is signaled by the `?stream=true` query parameter (WIRE_PROTOCOL §2.1).
     * @param {object} llmRequest
     * @param {{stream?: boolean}} [options]
     * @returns {Promise<object>}
     */
    llmCall(llmRequest, options = {}) {
      const path = options.stream ? `${WIRE.LLM_CALL}?stream=true` : WIRE.LLM_CALL;
      return request(path, { method: 'POST', body: llmRequest, signal: options.signal });
    },
    executeServerTool(toolCallEvent, options = {}) {
      return request(WIRE.TOOLS_EXECUTE, { method: 'POST', body: toolCallEvent, signal: options.signal });
    },
    pullTools() {
      return request(WIRE.TOOLS_LIST);
    },
    async postback(correlationId, postbackBody) {
      return request(
        `${WIRE.EVENTS}/${encodeURIComponent(correlationId)}/response`,
        { method: 'POST', body: postbackBody },
      );
    },
  };
}

module.exports = { createTransport };
