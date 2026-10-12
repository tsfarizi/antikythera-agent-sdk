/**
 * @antikythera/sdk - Antikythera Agent SDK
 *
 * General-purpose agent runtime with multi-agent orchestration,
 * session management, and MCP tool integration. Powered by WebAssembly.
 *
 * @packageDocumentation
 */

'use strict';

const fs = require('fs');
const path = require('path');

const { createAgentRuntime } = require('./runtime/index.js');
const { Orchestrator } = require('./orchestrator.js');
const { AgentDeployment, createAgentDeployment } = require('./delivery/index.js');


/**
 * @typedef {Object} AgentConfig
 * @property {string} provider - LLM provider identifier
 * @property {string} model - Model name
 * @property {string} [systemPrompt] - System prompt override
 * @property {number} [maxSteps] - Maximum reasoning steps
 * @property {number} [timeout] - Request timeout in ms
 */

/**
 * @typedef {Object} AgentResult
 * @property {string} output - Agent response text
 * @property {boolean} success - Execution success
 * @property {number} stepsUsed - Reasoning steps taken
 * @property {string} sessionId - Session identifier
 * @property {string} [error] - Error message if failed
 */

/**
 * @typedef {Object} AgentProfileConfig
 * @property {string} id - Unique agent ID
 * @property {string} name - Display name
 * @property {string} role - Semantic role
 * @property {string} [systemPrompt] - System prompt
 * @property {number} [maxSteps] - Max steps
 */

/**
 * @typedef {Object} TaskResult
 * @property {string} taskId - Task identifier
 * @property {string} agentId - Agent identifier
 * @property {*} output - Task output
 * @property {boolean} success - Task success
 * @property {number} stepsUsed - Steps taken
 * @property {string} sessionId - Session ID
 * @property {string} [error] - Error message
 * @property {string} [errorKind] - Error classification
 * @property {number} durationMs - Execution time
 */

/**
 * @typedef {Object} PipelineResult
 * @property {TaskResult[]} results - Individual results
 * @property {*} finalOutput - Final output
 * @property {number} totalSteps - Total steps
 * @property {boolean} success - Overall success
 * @property {string} [error] - Error message
 */

/**
 * @typedef {Object} OrchestratorConfig
 * @property {string} [executionMode] - Execution mode
 * @property {number} [maxConcurrentTasks] - Max concurrent tasks
 * @property {number} [maxTotalSteps] - Max total steps
 * @property {number} [maxTotalTasks] - Max total tasks
 * @property {string} [defaultRetryCondition] - Retry policy
 */

/**
 * @typedef {Object} SessionInfo
 * @property {string} sessionId - Session ID
 * @property {string} agentId - Agent ID
 * @property {number} createdAt - Creation timestamp
 * @property {number} lastActivity - Last activity timestamp
 * @property {number} messageCount - Message count
 */

/**
 * @typedef {Object} PromptConfig
 * @property {string} id - Unique prompt identifier
 * @property {string} name - Human-readable name
 * @property {string} content - The prompt content
 * @property {string} [description] - What this prompt does
 * @property {string[]} [tags] - Tags for categorization
 */


/**
 * Centralized prompt management for agents.
 *
 * Provides a registry for storing, organizing, and retrieving prompts.
 * Start with an empty registry and register your own prompts.
 *
 * @example
 * ```javascript
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
class PromptManager {
  /** @type {Map<string, PromptConfig>} */
  #prompts = new Map();

  /**
   * Create a new PromptManager.
   */
  constructor() {}

  /**
   * Register a prompt configuration.
   * @param {PromptConfig} config - Prompt configuration
   * @throws {Error} If prompt ID already exists
   */
  register(config) {
    if (!config.id || !config.content) {
      throw new Error('Prompt requires id and content');
    }
    if (this.#prompts.has(config.id)) {
      throw new Error(`Prompt '${config.id}' already exists. Use update() to modify.`);
    }
    this.#prompts.set(config.id, { ...config });
  }

  /**
   * Update an existing prompt.
   * @param {string} id - Prompt ID
   * @param {Partial<PromptConfig>} updates - Fields to update
   * @throws {Error} If prompt not found
   */
  update(id, updates) {
    const existing = this.#prompts.get(id);
    if (!existing) {
      throw new Error(`Prompt '${id}' not found`);
    }
    this.#prompts.set(id, { ...existing, ...updates, id });
  }

  /**
   * Get a prompt by ID.
   * @param {string} id - Prompt ID
   * @returns {PromptConfig | undefined} Prompt configuration
   */
  get(id) {
    return this.#prompts.get(id);
  }

  /**
   * Get all prompts with a specific tag.
   * @param {string} tag - Tag to filter by
   * @returns {PromptConfig[]} Matching prompts
   */
  getByTag(tag) {
    return [...this.#prompts.values()].filter(p => p.tags?.includes(tag));
  }

  /**
   * Get all registered prompts.
   * @returns {PromptConfig[]} All prompts
   */
  list() {
    return [...this.#prompts.values()];
  }

  /**
   * Check if a prompt exists.
   * @param {string} id - Prompt ID
   * @returns {boolean} Whether prompt exists
   */
  has(id) {
    return this.#prompts.has(id);
  }

  /**
   * Remove a prompt.
   * @param {string} id - Prompt ID
   * @returns {boolean} Whether prompt was removed
   */
  remove(id) {
    return this.#prompts.delete(id);
  }

  /**
   * Get prompt content by ID.
   * @param {string} id - Prompt ID
   * @returns {string | undefined} Prompt content
   */
  getContent(id) {
    return this.#prompts.get(id)?.content;
  }

  /**
   * Export all prompts as JSON.
   * @returns {string} JSON string
   */
  export() {
    return JSON.stringify([...this.#prompts.values()], null, 2);
  }

  /**
   * Import prompts from JSON.
   * @param {string} json - JSON string of prompts
   */
  import(json) {
    const prompts = JSON.parse(json);
    for (const prompt of prompts) {
      this.#prompts.set(prompt.id, prompt);
    }
  }

  /**
   * Create a PromptManager from a JSON file.
   * @param {string} filePath - Path to JSON file
   * @returns {PromptManager} New PromptManager instance
   */
  static fromFile(filePath) {
    const manager = new PromptManager();
    const content = fs.readFileSync(filePath, 'utf-8');
    manager.import(content);
    return manager;
  }

  /**
   * Create a PromptManager from a JSON string.
   * @param {string} json - JSON string
   * @returns {PromptManager} New PromptManager instance
   */
  static fromJSON(json) {
    const manager = new PromptManager();
    manager.import(json);
    return manager;
  }
}


/**
 * Session lifecycle management.
 */
class SessionManager {
  /** @type {Map<string, SessionInfo>} */
  #sessions = new Map();

  /** @type {number} */
  #maxSessions;

  /** @type {number} */
  #ttlMs;

  /**
   * Create a new SessionManager.
   * @param {{ maxSessions?: number, sessionTtlMs?: number }} [config] - Configuration
   */
  constructor(config = {}) {
    this.#maxSessions = config.maxSessions ?? 100;
    this.#ttlMs = config.sessionTtlMs ?? 3600000;
  }

  /**
   * Get or create a session.
   * @param {string} sessionId - Session ID
   * @param {string} agentId - Agent ID
   * @returns {SessionInfo} Session info
   */
  getOrCreate(sessionId, agentId) {
    const now = Date.now();
    let session = this.#sessions.get(sessionId);

    if (session) {
      session.lastActivity = now;
      return session;
    }

    if (this.#sessions.size >= this.#maxSessions) {
      this.#evictExpired();
      if (this.#sessions.size >= this.#maxSessions) {
        const oldest = [...this.#sessions.values()]
          .sort((a, b) => a.lastActivity - b.lastActivity)[0];
        if (oldest) {
          this.#sessions.delete(oldest.sessionId);
        }
      }
    }

    session = {
      sessionId,
      agentId,
      createdAt: now,
      lastActivity: now,
      messageCount: 0
    };

    this.#sessions.set(sessionId, session);
    return session;
  }

  /**
   * Get a session by ID.
   * @param {string} sessionId - Session ID
   * @returns {SessionInfo | null} Session info or null
   */
  get(sessionId) {
    return this.#sessions.get(sessionId) ?? null;
  }

  /**
   * List sessions for an agent.
   * @param {string} agentId - Agent ID
   * @returns {SessionInfo[]} Sessions
   */
  listByAgent(agentId) {
    return [...this.#sessions.values()].filter(s => s.agentId === agentId);
  }

  /**
   * List all sessions.
   * @returns {SessionInfo[]} All sessions
   */
  listAll() {
    return [...this.#sessions.values()];
  }

  /**
   * Remove a session.
   * @param {string} sessionId - Session ID
   * @returns {SessionInfo | null} Removed session or null
   */
  remove(sessionId) {
    const session = this.#sessions.get(sessionId);
    if (session === undefined) return null;
    this.#sessions.delete(sessionId);
    return session;
  }

  /**
   * Get session count.
   * @returns {number} Count
   */
  count() {
    return this.#sessions.size;
  }

  /** @private */
  #evictExpired() {
    const now = Date.now();
    for (const [id, session] of this.#sessions) {
      if (now - session.lastActivity > this.#ttlMs) {
        this.#sessions.delete(id);
      }
    }
  }
}


/**
 * Get the SDK version.
 *
 * The return line below is the version-bearing marker for
 * `src/scripts/sync-release-version.rs` (quoted 1.x literal) — keep it a string
 * literal, not a package.json lookup, and never quote the marker elsewhere
 * in this file: the sync script rewrites the FIRST matching line.
 * @returns {string} Version string
 */
function getVersion() {
  return "1.8.5";
}

module.exports = {
  PromptManager,
  SessionManager,
  createAgentRuntime,
  Orchestrator,
  AgentDeployment,
  createAgentDeployment,
  getVersion
};
