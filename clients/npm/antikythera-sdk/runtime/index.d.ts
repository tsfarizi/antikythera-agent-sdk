/**
 * Antikythera runtime bridge — high-level host runtime for the WASM agent
 * core. Wire contract: `documentation/WIRE_PROTOCOL.md`.
 */

import type { ToolEntry, ToolResultInput } from './types';
import type { RuntimeHooks } from './control';
import type { PermissionPolicy } from './policy';

export { createClientCoreRuntime } from './runner-core';
export { createServerCoreRuntime } from './peer-core';
export { createPolicyGate, PermissionPolicy, PolicyGate } from './policy';
export { createUnionRegistry, UnionRegistry, ToolOwner } from './registry';
export { createTransport, Transport } from './transport';
export { createSseChannel, SseChannel, SseEnvelope, SseStatus } from './sse';
export {
  createControlHandler,
  ControlHandler,
  ControlHandlerOptions,
  PermissionGate,
  installRuntimeHooksProvider,
  acquireRuntimeHooksProvider,
  releaseRuntimeHooksProvider,
  wrapHookFunction,
  invokeHook,
  RuntimeHooks,
} from './control';
export {
  WIRE,
  joinUrl,
  randomId,
  buildLlmRequest,
  LlmRequest,
  LlmRequestInput,
  parseLlmResponse,
  LlmResponse,
  buildToolCallEvent,
  ToolCallEvent,
  ToolCallEventInput,
  parseToolExecutionResult,
  ToolExecutionResultWire,
  wireToRunnerToolResult,
  ToolResultInput,
  parseEventEnvelope,
  WireEventEnvelope,
  buildPostback,
  Postback,
  PostbackInput,
  ToolDefinition,
  ToolParameterSchema,
  ToolEntry,
  ToolHandler,
  ToolHandlerResult,
  normalizeLocalTools,
  runToolHandler,
} from './types';

/** Where the WASM runner lives. */
export type CoreMode = 'client' | 'server';

/**
 * The jco `runner` namespace: the 16 camelCase functions exported by the
 * component bundle (`antikythera-agent/component`).
 */
export type RunnerNamespace = typeof import('../component/interfaces/antikythera-agent-sdk-runner');

export interface LlmOptions {
  provider?: string | null;
  model?: string | null;
  temperature?: number | null;
  maxTokens?: number | null;
  schemaName?: string | null;
  metadataJson?: string | null;
  /** Request LLM streaming; tokens arrive as `llm-token` SSE events (default true). */
  stream?: boolean;
}

export interface AgentRuntimeOptionsBase {
  /** Base URL of the Antikythera server. */
  serverUrl: string;
  authToken?: string;
  timeoutMs?: number;
  /** Opaque client identifier for the SSE control channel. */
  clientId?: string;
  /** Session id for the client core (default: runner-generated). */
  sessionId?: string;
  /** Client-owned tool definitions + handlers (locked to the client). */
  tools?: ToolEntry[];
  /** Runtime hooks provider (prepareTurn/decideAction/handleToolResult). */
  hooks?: RuntimeHooks;
  /** Client-side permission policy; default-deny allowlist. */
  policy?: PermissionPolicy;
  /** Runner max_steps (default 10). */
  maxSteps?: number;
  /** Default system prompt for turns. */
  systemPrompt?: string;
  /** Prompt used for loop iterations after a tool result (default '[continue]'). */
  continuationPrompt?: string;
  /** Session timeout in seconds (runner config). */
  sessionTimeoutSecs?: number;
  /** Max in-memory sessions (runner config). */
  maxInMemorySessions?: number;
  /** Context policy (runner config). */
  contextPolicy?: Record<string, unknown>;
}

export interface ClientCoreOptions extends AgentRuntimeOptionsBase {
  core?: 'client';
  llm?: LlmOptions;
  /**
   * Absolute URL of the jco bundle directory; the entry file is resolved from
   * the server manifest (`GET /antikythera/v1/component/manifest`, WIRE_PROTOCOL
   * §2.6). Omit to keep the bundled component (default, decision D5).
   */
  componentBase?: string;
  /**
   * Directly injected runner namespace; bypasses the component import
   * entirely (decision D5). Takes precedence over `componentBase`.
   */
  runner?: RunnerNamespace;
}

export interface ServerCoreOptions extends AgentRuntimeOptionsBase {
  core: 'server';
}

export type AgentRuntimeOptions = ClientCoreOptions | ServerCoreOptions;

/** A runner event drained from the WASM session (kind is snake_case). */
export interface RunnerEvent {
  seq: number;
  session_id: string;
  step: number;
  correlation_id: string | null;
  kind: string;
  payload: Record<string, unknown>;
}

export interface TurnOptions {
  signal?: AbortSignal;
  maxSteps?: number;
  systemPrompt?: string;
  forceJson?: boolean;
  metadataJson?: string | null;
  correlationId?: string;
  continuationPrompt?: string;
}

export interface TurnResult {
  sessionId: string;
  action: 'final';
  content: string | null;
  events: RunnerEvent[];
  iterations: number;
}

/** Runtime event delivered to `onEvent` listeners. */
export interface RuntimeEvent {
  type: string;
  [key: string]: unknown;
}

/** Common surface of both core modes. */
export interface AgentRuntime {
  readonly core: CoreMode;
  readonly serverUrl: string;
  readonly clientId: string;
  readonly sessionId: string | null;
  readonly connected: boolean;
  connect(): Promise<void>;
  close(): void;
  onEvent(listener: (event: RuntimeEvent) => void): () => void;
}

/** Client-core runtime: owns the WASM runner and the tool loop. */
export interface ClientCoreRuntime extends AgentRuntime {
  cancelTurn(): boolean;
  /** Auto tool-owner loop: prepare -> LLM proxy -> commit -> drain -> route -> process -> final. */
  runTurn(prompt: string, opts?: TurnOptions): Promise<TurnResult>;
  /** Execute one tool (local gated / server via /tools/execute); returns runner ToolResultInput. */
  executeTool(toolName: string, args?: Record<string, unknown>): Promise<ToolResultInput>;
  getState(): Record<string, unknown>;
  getToolsPrompt(): string;
  resetSession(): boolean;
  /** Re-pull the server registry, recompute the union, re-register. */
  refreshTools(): Promise<void>;
}

/** Server-core runtime: control-channel peer, no runner. */
export interface ServerCoreRuntime extends AgentRuntime {
  /** Execute a client-owned tool (gate + handler). */
  executeLocalTool(toolName: string, args?: Record<string, unknown>): Promise<ToolResultInput>;
}

export function createAgentRuntime(options: ClientCoreOptions): Promise<ClientCoreRuntime>;
export function createAgentRuntime(options: ServerCoreOptions): Promise<ServerCoreRuntime>;
export function createAgentRuntime(options: AgentRuntimeOptions): Promise<AgentRuntime>;
