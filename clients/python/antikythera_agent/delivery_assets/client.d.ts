/** Standalone declarations copied to agent.d.ts in every configured client export. */
export interface DeliveredAgentResult {
  output: string;
  success: boolean;
  stepsUsed: number;
  sessionId: string;
  events: unknown[];
}

/** A private client instance; cancellation never commits a late provider response. */
export interface DeliveredAgentRuntime {
  readonly sessionId: string;
  run(
    prompt: string,
    options?: { signal?: AbortSignal },
  ): Promise<DeliveredAgentResult>;
  getState(): Record<string, unknown>;
  close(): void;
}

/** Capabilities supplied by the client application instead of serialized host closures. */
export interface ClientEnvironment {
  serverUrl?: string;
  authToken?: string;
  provider?: (
    request: {
      provider: string;
      model: string;
      session_id: string;
      messages_json: string;
      force_json: boolean;
      temperature: number | null;
      max_tokens: number | null;
      schema_name: string | null;
      metadata_json: string | null;
    },
    options: { signal: AbortSignal },
  ) => { content: string } | Promise<{ content: string }>;
  tools?: Record<
    string,
    (
      input: unknown,
      options: { signal: AbortSignal },
    ) => unknown | Promise<unknown>
  >;
  hooks?: {
    prepareTurn?: (
      requestJson: string,
      stateJson: string,
    ) => string | Record<string, unknown>;
    decideAction?: (
      stateJson: string,
      responseJson: string,
    ) => string | Record<string, unknown>;
    handleToolResult?: (
      stateJson: string,
      resultJson: string,
    ) => string | Record<string, unknown>;
  };
}

/** Public agent metadata mirrored by agent.json; changing this object does not update a running instance. */
export const definition: {
  schemaVersion: 1;
  sdkVersion: string;
  serverUrl: string | null;
  agent: {
    id: string;
    provider: string;
    model: string;
    systemPrompt: string | null;
    maxSteps: number;
    timeoutMs: number;
    hooksModule: string | null;
    tools: Array<{
      definition: Record<string, unknown>;
      allow: boolean;
      execution: "server" | "client";
      module: string | null;
    }>;
  };
};

/** Instantiate the delivered WASM/jco environment with optional instance-owned host bindings. */
export function createAgent(
  environment?: ClientEnvironment,
): Promise<DeliveredAgentRuntime>;
