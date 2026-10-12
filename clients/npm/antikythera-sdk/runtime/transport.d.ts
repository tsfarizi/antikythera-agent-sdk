/**
 * HTTP transport for the wire protocol. All requests target
 * `${serverUrl}/antikythera/v1/*`; the client never fetches an LLM provider
 * directly (invariant R6).
 */

export interface LlmCallOptions {
  /** Signal LLM streaming via the `?stream=true` query parameter (WIRE_PROTOCOL §2.1). */
  stream?: boolean; signal?: AbortSignal;
}

export interface Transport {
  /** POST /antikythera/v1/llm/call — proxy an LLM call; streaming is a query parameter. */
  llmCall(llmRequest: object, options?: LlmCallOptions): Promise<object>;
  /** POST /antikythera/v1/tools/execute — execute a server-/mcp-owned tool. */
  executeServerTool(toolCallEvent: object, options?: { signal?: AbortSignal }): Promise<object>;
  /** GET /antikythera/v1/tools — registry pull (C1). */
  pullTools(): Promise<object[]>;
  /** POST /antikythera/v1/events/{correlation-id}/response — POST-back. */
  postback(correlationId: string, postbackBody: object): Promise<null>;
}

export function createTransport(options: { serverUrl: string; authToken?: string; timeoutMs?: number; clientId?: string }): Transport;
