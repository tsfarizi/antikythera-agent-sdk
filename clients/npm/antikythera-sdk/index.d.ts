/**
 * @antikythera/sdk - Antikythera Agent SDK
 *
 * Agent runtime with multi-agent orchestration, session management,
 * and MCP tool integration. Powered by WebAssembly.
 *
 * @packageDocumentation
 */


/**
 * Configuration for creating an Agent instance.
 */
export { AgentDeployment, createAgentDeployment } from './delivery/index';
export type { DeploymentConfig, DeploymentEnvironment, ClientExportOptions, DeliveryHostOptions, DeliveryHost, DeliveryManifest, ConfiguredAgentRuntime, DeliveryResult } from './delivery/index';

export interface AgentConfig {
  /** LLM provider identifier (e.g., 'openai', 'anthropic', 'gemini') */
  provider: string;
  /** Model name to use (e.g., 'gpt-4o', 'claude-3-opus') */
  model: string;
  /** Optional system prompt override */
  systemPrompt?: string;
  /** Maximum reasoning steps before forced completion */
  maxSteps?: number;
  /** Request timeout in milliseconds */
  timeout?: number;
}

/**
 * Result from an agent execution.
 */
export interface AgentResult {
  /** The agent's response text */
  output: string;
  /** Whether the execution completed successfully */
  success: boolean;
  /** Number of reasoning steps taken */
  stepsUsed: number;
  /** The session identifier for this conversation */
  sessionId: string;
  /** Error message if execution failed */
  error?: string;
}

/**
 * Configuration for an agent profile in multi-agent orchestration.
 */
export interface AgentProfileConfig {
  /** Unique identifier for this agent */
  id: string;
  /** Human-readable display name */
  name: string;
  /** Semantic role label (e.g., 'coder', 'reviewer', 'analyst') */
  role: string;
  /** System prompt defining this agent's behavior */
  systemPrompt?: string;
  /** Maximum reasoning steps for this agent */
  maxSteps?: number;
}

/**
 * Result from a single task in multi-agent orchestration.
 */
export interface TaskResult {
  /** Unique identifier for the task */
  taskId: string;
  /** ID of the agent that executed the task */
  agentId: string;
  /** The task output as a JSON value */
  output: unknown;
  /** Whether the task completed successfully */
  success: boolean;
  /** Number of reasoning steps taken */
  stepsUsed: number;
  /** Session identifier */
  sessionId: string;
  /** Error message if task failed */
  error?: string;
  /** Error classification */
  errorKind?: 'transient' | 'permanent' | 'cancelled';
  /** Execution time in milliseconds */
  durationMs: number;
}

/**
 * Result from a pipeline of sequential tasks.
 */
export interface PipelineResult {
  /** Individual task results */
  results: TaskResult[];
  /** The final output from the last task */
  finalOutput: unknown;
  /** Total steps across all tasks */
  totalSteps: number;
  /** Whether all tasks completed successfully */
  success: boolean;
  /** Error message if any task failed */
  error?: string;
}

/**
 * Configuration for the multi-agent orchestrator.
 */
export interface OrchestratorConfig {
  /** How tasks are executed */
  executionMode?: 'auto' | 'sequential' | 'concurrent' | 'parallel';
  /** Maximum tasks running simultaneously */
  maxConcurrentTasks?: number;
  /** Maximum total steps across all tasks */
  maxTotalSteps?: number;
  /** Maximum number of tasks */
  maxTotalTasks?: number;
  /** Retry policy */
  defaultRetryCondition?: 'always' | 'on-transient' | 'never';
}

/**
 * Session information.
 */
export interface SessionInfo {
  /** Unique session identifier */
  sessionId: string;
  /** Agent associated with this session */
  agentId: string;
  /** Creation timestamp (Unix milliseconds) */
  createdAt: number;
  /** Last activity timestamp (Unix milliseconds) */
  lastActivity: number;
  /** Number of messages in this session */
  messageCount: number;
}

/**
 * Prompt configuration for centralized prompt management.
 */
export interface PromptConfig {
  /** Unique prompt identifier */
  id: string;
  /** Human-readable name */
  name: string;
  /** The prompt content */
  content: string;
  /** What this prompt does */
  description?: string;
  /** Tags for categorization */
  tags?: string[];
}


/**
 * Centralized prompt management for agents.
 *
 * Provides a registry for storing, organizing, and retrieving prompts.
 * Start with an empty registry and register your own prompts.
 *
 * @example
 * ```typescript
 * const prompts = new PromptManager();
 *
 * prompts.register({
 *   id: 'my-agent',
 *   name: 'My Agent',
 *   content: 'You are a helpful assistant.',
 *   tags: ['general']
 * });
 *
 * const content = prompts.getContent('my-agent');
 * ```
 */
export class PromptManager {
  /**
   * Create a new PromptManager.
   */
  constructor();

  /**
   * Register a prompt configuration.
   * @param config - Prompt configuration
   * @throws Error if prompt ID already exists
   */
  register(config: PromptConfig): void;

  /**
   * Update an existing prompt.
   * @param id - Prompt ID
   * @param updates - Fields to update
   * @throws Error if prompt not found
   */
  update(id: string, updates: Partial<PromptConfig>): void;

  /**
   * Get a prompt by ID.
   * @param id - Prompt ID
   * @returns Prompt configuration or undefined
   */
  get(id: string): PromptConfig | undefined;

  /**
   * Get all prompts with a specific tag.
   * @param tag - Tag to filter by
   * @returns Matching prompts
   */
  getByTag(tag: string): PromptConfig[];

  /**
   * Get all registered prompts.
   * @returns All prompts
   */
  list(): PromptConfig[];

  /**
   * Check if a prompt exists.
   * @param id - Prompt ID
   * @returns Whether prompt exists
   */
  has(id: string): boolean;

  /**
   * Remove a prompt.
   * @param id - Prompt ID
   * @returns Whether prompt was removed
   */
  remove(id: string): boolean;

  /**
   * Get prompt content by ID.
   * @param id - Prompt ID
   * @returns Prompt content or undefined
   */
  getContent(id: string): string | undefined;

  /**
   * Export all prompts as JSON.
   * @returns JSON string
   */
  export(): string;

  /**
   * Import prompts from JSON.
   * @param json - JSON string of prompts
   */
  import(json: string): void;

  /**
   * Create a PromptManager from a JSON file.
   * @param filePath - Path to JSON file
   * @returns New PromptManager instance
   */
  static fromFile(filePath: string): PromptManager;

  /**
   * Create a PromptManager from a JSON string.
   * @param json - JSON string
   * @returns New PromptManager instance
   */
  static fromJSON(json: string): PromptManager;
}


/**
 * Session lifecycle management.
 */
export class SessionManager {
  /**
   * Create a new SessionManager.
   * @param config - Configuration
   */
  constructor(config?: { maxSessions?: number; sessionTtlMs?: number });

  /**
   * Get or create a session.
   * @param sessionId - Session ID
   * @param agentId - Agent ID
   * @returns Session information
   */
  getOrCreate(sessionId: string, agentId: string): SessionInfo;

  /**
   * Get a session by ID.
   * @param sessionId - Session identifier
   * @returns Session information or null
   */
  get(sessionId: string): SessionInfo | null;

  /**
   * List all sessions for an agent.
   * @param agentId - Agent identifier
   * @returns Array of session information
   */
  listByAgent(agentId: string): SessionInfo[];

  /**
   * List all sessions.
   * @returns Array of session information
   */
  listAll(): SessionInfo[];

  /**
   * Remove a session.
   * @param sessionId - Session identifier
   * @returns Removed session or null
   */
  remove(sessionId: string): SessionInfo | null;

  /**
   * Get total session count.
   * @returns Number of active sessions
   */
  count(): number;
}


/**
 * Multi-agent orchestrator on top of client-core runtimes.
 *
 * One runtime per agent profile (session-per-profile); the runtime seam is
 * injectable via `runtimeFactory` (default: `createAgentRuntime({core:'client'})`
 * bound to `serverUrl`). Runtimes for all registered profiles are acquired on
 * the first dispatched task.
 */
export class Orchestrator {
  /**
   * Create an orchestrator.
   * @param config - Orchestrator configuration; `serverUrl` is required
   * @throws Error if serverUrl is missing or empty
   */
  constructor(
    config: OrchestratorConfig & {
      serverUrl: string;
      runtimeFactory?: (cfg: ClientCoreOptions) => Promise<ClientCoreRuntime>;
    }
  );

  /**
   * Register an agent profile (stored as a defensive copy).
   * @param profile - Agent profile
   * @throws Error if id, name, or role is missing/empty
   */
  registerAgent(profile: AgentProfileConfig): void;

  /**
   * All registered profiles (copies).
   * @returns Array of agent profiles
   */
  listAgents(): AgentProfileConfig[];

  /**
   * Dispatch one task to the default agent (first registered).
   * @param task - Task prompt
   * @param opts - Options; `sessionId` reuses the runtime owning that session
   * @returns Task result
   */
  dispatch(task: string, opts?: { sessionId?: string; agentId?: string }): Promise<TaskResult>;

  /**
   * Dispatch many tasks honouring the configured execution mode.
   * Results keep input order.
   * @param tasks - Task prompts
   * @param opts - Options forwarded per task (sequential mode)
   * @returns Array of task results
   */
  dispatchMany(tasks: string[], opts?: { sessionId?: string; agentId?: string }): Promise<TaskResult[]>;

  /**
   * Run tasks sequentially, feeding each previous output into the next
   * prompt. Short-circuits at the first failed task.
   * @param tasks - Task prompts
   * @returns Pipeline result
   */
  pipeline(tasks: string[]): Promise<PipelineResult>;

  /**
   * Factual budget snapshot (counters update after every executed result).
   * @returns Budget state
   */
  getBudget(): {
    consumedSteps: number;
    dispatchedTasks: number;
    isStepBudgetExhausted: boolean;
    isTaskBudgetExhausted: boolean;
  };

  /**
   * Reset the runner session(s). Idempotent; program errors propagate.
   * @param sessionId - Session to reset; omit to cancel every recorded session
   * @returns true when at least one session was removed
   */
  cancel(sessionId?: string): Promise<boolean>;

  /**
   * Close every live runtime (best-effort).
   */
  close(): void;
}


/**
 * Get the SDK version.
 * @returns Version string
 */
export function getVersion(): string;

/**
 * Runtime Bridge (U9) — host runtime for the WASM agent core.
 */

export { createAgentRuntime } from './runtime/index';
export type {
  AgentRuntime,
  AgentRuntimeOptions,
  AgentRuntimeOptionsBase,
  ClientCoreOptions,
  ClientCoreRuntime,
  CoreMode,
  LlmOptions,
  PermissionPolicy,
  RuntimeEvent,
  RuntimeHooks,
  RunnerEvent,
  ServerCoreOptions,
  ServerCoreRuntime,
  ToolDefinition,
  ToolEntry,
  ToolHandler,
  ToolHandlerResult,
  ToolParameterSchema,
  ToolResultInput,
  TurnOptions,
  TurnResult,
} from './runtime/index';
