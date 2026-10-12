/**
 * SSE control channel client for GET /antikythera/v1/events.
 */

export interface SseEnvelope {
  type: string;
  correlation_id: string | null;
  session_id: string | null;
  client_id: string | null;
  payload: unknown;
}

export type SseStatus =
  | { state: 'connecting' }
  | { state: 'connected' }
  | { state: 'closed' }
  | { state: 'reconnecting'; retries: number; delay: number }
  | { state: 'error'; error: Error };

export interface SseChannel {
  start(): void;
  ready(): Promise<void>;
  stop(): void;
}

export function createSseChannel(options: {
  url: string;
  headers?: Record<string, string>;
  onEvent?: (envelope: SseEnvelope, meta: { id?: string }) => void;
  onStatus?: (status: SseStatus) => void;
  reconnect?: boolean;
  timeoutMs?: number;
}): SseChannel;
