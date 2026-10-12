/** Server-side agent configuration and portable WASM Component Model delivery. */
import type { RuntimeHooks } from "../runtime/control";
import type { ToolDefinition, LlmRequest } from "../runtime/types";
import type { ToolRef } from "../runtime/tools";

/** Public configuration; credentials and executable bindings belong to DeploymentEnvironment. */
export interface DeploymentConfig {
  id: string;
  provider: string;
  model: string;
  systemPrompt?: string;
  maxSteps?: number;
  timeoutMs?: number;
  /** Accepts an existing AgentConfig timeout in milliseconds; timeoutMs takes precedence. */
  timeout?: number;
}

/** Server capabilities retained in memory and never serialized into the exported bundle. */
export interface DeploymentEnvironment {
  provider?: (
    request: LlmRequest,
    options: { signal?: AbortSignal },
  ) => { content: string } | Promise<{ content: string }>;
  tools?: Array<
    | {
        definition: ToolDefinition;
        handler: (
          input: unknown,
          options: { signal?: AbortSignal },
        ) => unknown | Promise<unknown>;
        allow?: boolean;
      }
    | { tool: ToolRef; allow?: boolean }
    | ToolRef
  >;
  hooks?: RuntimeHooks;
}

/** Optional JavaScript implementations copied from an explicitly selected binding directory. */
export interface ClientExportOptions {
  serverUrl?: string;
  bindingsDir?: string;
  clientTools?: Record<string, string>;
  hooksModule?: string;
}

/** Per-turn result from the private Component Model runner. */
export interface DeliveryResult {
  output: string;
  success: boolean;
  stepsUsed: number;
  sessionId: string;
  events?: unknown[];
  error?: string;
}

/** Lifecycle of one independent jco instance and its session. */
export interface ConfiguredAgentRuntime {
  readonly sessionId: string;
  run(
    prompt: string,
    options?: { signal?: AbortSignal },
  ): Promise<DeliveryResult>;
  getState(): Record<string, unknown>;
  close(): void;
}

/** Files needed to reproduce and independently check the delivered environment. */
export interface DeliveryManifest {
  schemaVersion: 1;
  sdkVersion: string;
  agentId: string;
  entry: string;
  configuration: string;
  component: string;
  jco: string;
  files: Array<{ path: string; size: number; sha256: string }>;
}

/** HTTP deployment host; publicUrl allows mounting behind a reverse proxy. */
export interface DeliveryHostOptions
  extends Omit<ClientExportOptions, "serverUrl"> {
  bind?: string;
  port?: number;
  authToken?: string;
  allowedOrigins?: string[];
  publicUrl?: string;
}

/** Running server with a public client entry and an explicit shutdown operation. */
export interface DeliveryHost {
  url: string;
  clientUrl: string;
  manifestUrl: string;
  close(): Promise<void>;
}

/** One configured agent that can run in Node.js or be exported for client execution. */
export class AgentDeployment {
  readonly definition: typeof import("./client").definition;
  constructor(config: DeploymentConfig, environment?: DeploymentEnvironment);
  createRuntime(): Promise<ConfiguredAgentRuntime>;
  run(
    prompt: string,
    options?: { signal?: AbortSignal },
  ): Promise<DeliveryResult>;
  exportClient(
    destination: string,
    options?: ClientExportOptions,
  ): Promise<DeliveryManifest>;
  serve(options?: DeliveryHostOptions): Promise<DeliveryHost>;
  close(): Promise<void>;
}

/** Creates a deployment without repeating its configuration for each execution target. */
export function createAgentDeployment(
  config: DeploymentConfig,
  environment?: DeploymentEnvironment,
): AgentDeployment;
