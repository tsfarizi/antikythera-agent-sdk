'use strict';

/**
 * SSE control channel client (GET /antikythera/v1/events). Streams the wire
 * event envelope to a handler, creates a fresh subscription on reconnect, and
 * re-registers `client_id` by keeping it in the request URL.
 */

/**
 * @typedef {{type: string, correlation_id: string|null, session_id: string|null, client_id: string|null, payload: any}} Envelope
 * @typedef {{state: string, retries?: number, delay?: number, error?: Error}} Status
 */

/**
 * @param {string} frame
 * @returns {{event?: string, data?: string, id?: string, retry?: string}|null}
 */
function parseFrame(frame) {
  const out = {};
  for (const line of frame.split('\n')) {
    if (line.startsWith(':')) continue;
    const colon = line.indexOf(':');
    const field = colon === -1 ? line : line.slice(0, colon);
    const value = colon === -1 ? '' : line.slice(colon + 1).replace(/^ /, '');
    if (field === 'event') out.event = value;
    else if (field === 'data') out.data = (out.data ? out.data + '\n' : '') + value;
    else if (field === 'id') out.id = value;
    else if (field === 'retry') out.retry = value;
  }
  if (out.event === undefined && out.data === undefined && out.id === undefined) {
    return null;
  }
  return out;
}

/**
 * Open an SSE control channel and expose its connection readiness promise.
 * Reconnection creates a fresh subscription without replaying earlier frames.
 * start() callers receive connection errors through onStatus even when they
 * do not await ready; await-ready callers receive the same failure as rejection.
 * @param {object} options
 * @param {string} options.url - full events URL including client_id (+ optional session_id)
 * @param {object} [options.headers] - extra request headers
 * @param {(envelope: Envelope, meta: {id?: string}) => void} [options.onEvent]
 * @param {(status: Status) => void} [options.onStatus]
 * @param {boolean} [options.reconnect] - resume with backoff on drop (default true)
 * @returns {{ start: () => void, stop: () => void, ready: Promise<void> }}
 */
function createSseChannel(options) {
  const url = options.url;
  const extraHeaders = options.headers ?? {};
  const onEvent = options.onEvent ?? (() => {});
  const onStatus = options.onStatus ?? (() => {});
  const reconnectEnabled = options.reconnect !== false;
  const maxBackoffMs = 10000;

  let controller = null;
  let stopped = false;
  let retries = 0;
  let timer = null;
  let readyTimer = null;
  let resolveReady, rejectReady;
  const ready = new Promise((resolve, reject) => { resolveReady = resolve; rejectReady = reject; });
  ready.catch(() => {});

  async function pump() {
    onStatus({ state: 'connecting' });
    const headers = { Accept: 'text/event-stream', ...extraHeaders };
    try {
      const response = await fetch(url, { headers, signal: controller.signal });
      if (!response.ok) {
        throw new Error(`SSE HTTP ${response.status}`);
      }
      if (!response.body) {
        throw new Error('SSE: response has no body');
      }
      retries = 0;
      clearTimeout(readyTimer);
      resolveReady();
      onStatus({ state: 'connected' });
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const frames = buffer.split('\n\n');
        buffer = frames.pop() ?? '';
        for (const frame of frames) {
          const parsed = parseFrame(frame);
          if (!parsed) continue;
          if (parsed.data) {
            let envelope;
            try {
              envelope = JSON.parse(parsed.data);
            } catch {
              envelope = { type: parsed.event ?? 'message', payload: parsed.data };
            }
            if (typeof envelope.type !== 'string') {
              envelope.type = parsed.event ?? 'message';
            }
            onEvent(envelope, { id: parsed.id });
          }
        }
      }
      onStatus({ state: 'closed' });
    } catch (err) {
      clearTimeout(readyTimer);
      rejectReady(err);
      if (stopped || err.name === 'AbortError') return;
      onStatus({ state: 'error', error: err });
    }
    if (stopped || !reconnectEnabled) return;
    retries += 1;
    const delay = Math.min(500 * 2 ** Math.min(retries - 1, 6), maxBackoffMs);
    onStatus({ state: 'reconnecting', retries, delay });
    timer = setTimeout(() => {
      if (!stopped) pump();
    }, delay);
  }

  return {
    start() {
      if (controller) return;
      stopped = false;
      controller = new AbortController();
      readyTimer = setTimeout(() => rejectReady(new Error('SSE connection timed out')), options.timeoutMs ?? 10000);
      pump();
    },
    ready() { return ready; },
    stop() {
      stopped = true;
      clearTimeout(readyTimer);
      rejectReady(new Error('SSE channel stopped'));
      if (timer) clearTimeout(timer);
      if (controller) controller.abort();
      controller = null;
    },
  };
}

module.exports = { createSseChannel };
