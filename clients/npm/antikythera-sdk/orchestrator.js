'use strict';

/**
 * Orchestrator — multi-agent task orchestration on top of client-core
 * runtimes (debug/U1-design-notes.md §E3).
 *
 * Topology: one runtime per agent profile (session-per-profile). The runtime
 * seam is injectable via `config.runtimeFactory(config)`; the default builds
 * a client-core runtime bound to `config.serverUrl`. Runtimes for all
 * registered profiles are acquired on the first dispatched task so any
 * recorded session can be cancelled deterministically afterwards.
 */

const { createAgentRuntime } = require('./runtime/index.js');

/**
 * @typedef {import('./runtime/index.js').ClientCoreRuntime} ClientCoreRuntime
 */

/**
 * Multi-agent orchestrator above `createAgentRuntime({core:'client'})`.
 */
class Orchestrator {
  /** @type {string} */
  #serverUrl;
  /** @type {'auto'|'sequential'|'concurrent'|'parallel'} */
  #executionMode;
  /** @type {number} */
  #maxConcurrentTasks;
  /** @type {number|null} */
  #maxTotalSteps;
  /** @type {number|null} */
  #maxTotalTasks;
  /** @type {string} */
  #defaultRetryCondition;
  /** @type {(cfg: object) => Promise<ClientCoreRuntime>} */
  #runtimeFactory;
  /** @type {Array<AgentProfileConfig>} */
  #profiles;
  /** @type {Map<number, Promise<ClientCoreRuntime>>} memoized per-profile acquisition */
  #profileRuntimes;
  /** @type {Map<string, ClientCoreRuntime>} sessionId -> owning runtime */
  #sessions;
  /** @type {Set<string>} session ids already reset (cancel idempotency) */
  #cancelled;
  /** @type {Set<ClientCoreRuntime>} resolved runtimes alive in this pool */
  #liveRuntimes;
  /** @type {{consumedSteps: number, dispatchedTasks: number}} */
  #budget;
  /** @type {number} */
  #taskSeq;
  #reservedSteps = 0;
  #inFlightTasks = 0;

  /**
   * @param {object} config
   * @param {string} config.serverUrl - base URL of the Antikythera server (WAJIB)
   * @param {'auto'|'sequential'|'concurrent'|'parallel'} [config.executionMode='auto']
   * @param {number} [config.maxConcurrentTasks=4]
   * @param {number} [config.maxTotalSteps]
   * @param {number} [config.maxTotalTasks]
   * @param {'always'|'on-transient'|'never'} [config.defaultRetryCondition='always']
   * @param {(cfg: object) => Promise<ClientCoreRuntime>} [config.runtimeFactory]
   */
  constructor(config = {}) {
    if (typeof config.serverUrl !== 'string' || config.serverUrl.length === 0) {
      throw new Error('Orchestrator: config.serverUrl is required (non-empty string)');
    }
    const mode = config.executionMode ?? 'auto';
    if (!['auto', 'sequential', 'concurrent', 'parallel'].includes(mode)) {
      throw new Error(`Orchestrator: unknown executionMode '${mode}'`);
    }
    const maxConcurrent = config.maxConcurrentTasks ?? 4;
    if (!Number.isInteger(maxConcurrent) || maxConcurrent < 1) {
      throw new Error('Orchestrator: maxConcurrentTasks must be a positive integer');
    }
    for (const name of ['maxTotalTasks','maxTotalSteps']) {
      if (config[name] != null && (!Number.isInteger(config[name]) || config[name] < 0)) throw new Error(`Orchestrator: ${name} must be a non-negative integer`);
    }
    this.#serverUrl = config.serverUrl;
    this.#executionMode = mode;
    this.#maxConcurrentTasks = maxConcurrent;
    this.#maxTotalSteps = config.maxTotalSteps ?? null;
    this.#maxTotalTasks = config.maxTotalTasks ?? null;
    this.#defaultRetryCondition = config.defaultRetryCondition ?? 'always';
    this.#runtimeFactory = config.runtimeFactory ?? ((cfg) => createAgentRuntime({ core: 'client', ...cfg }));
    this.#profiles = [];
    this.#profileRuntimes = new Map();
    this.#sessions = new Map();
    this.#cancelled = new Set();
    this.#liveRuntimes = new Set();
    this.#budget = { consumedSteps: 0, dispatchedTasks: 0 };
    this.#taskSeq = 0;
  }

  /**
   * Register an agent profile. The stored profile is a defensive copy.
   * @param {{id: string, name: string, role: string, systemPrompt?: string, maxSteps?: number}} profile
   */
  registerAgent(profile) {
    if (
      !profile ||
      typeof profile.id !== 'string' || profile.id.length === 0 ||
      typeof profile.name !== 'string' || profile.name.length === 0 ||
      typeof profile.role !== 'string' || profile.role.length === 0
    ) {
      throw new Error('Orchestrator: agent profile requires non-empty id, name, and role');
    }
    if (this.#profiles.some(existing => existing.id === profile.id)) throw new Error('Orchestrator: duplicate agent id');
    if (profile.maxSteps != null && (!Number.isInteger(profile.maxSteps) || profile.maxSteps < 1)) throw new Error('Orchestrator: profile maxSteps must be a positive integer');
    this.#profiles.push({ ...profile });
  }

  /**
   * All registered profiles (copies; mutations do not leak into the registry).
   * @returns {Array<{id: string, name: string, role: string, systemPrompt?: string, maxSteps?: number}>}
   */
  listAgents() {
    return this.#profiles.map((profile) => ({ ...profile }));
  }

  /**
   * Dispatch one task to the default agent (first registered).
   * @param {string} task
   * @param {{sessionId?: string}} [opts] - reuse the runtime owning that session
   * @returns {Promise<{taskId: string, agentId: string, output: *, success: boolean,
   *   stepsUsed: number, sessionId: string, error?: string, errorKind?: string, durationMs: number}>}
   */
  async dispatch(task, opts = {}) {
    const profile = this.#selectProfile(opts.agentId);
    if (this.#isBudgetExhausted()) {
      return this.#budgetDenied(profile.id);
    }
    const cached = typeof opts?.sessionId === 'string' ? this.#sessions.get(opts.sessionId) : undefined;
    const runtimeSource = cached ? Promise.resolve(cached) : () => this.#ensureProfileRuntime(this.#profiles.indexOf(profile));
    return this.#runTask(profile, runtimeSource, task);
  }

  /**
   * Dispatch many tasks honouring executionMode.
   *
   * sequential / auto(1 task): for..await over the shared per-profile runtime
   * (session-per-profile persistence). concurrent | parallel | auto(n>1):
   * bounded worker pool with one fresh isolated runtime per in-flight task
   * (E3.1 isolation — concurrent runTurn on one client-core runtime would
   * interleave prepare/drain on a single sessionId). Results keep input order.
   *
   * @param {string[]} tasks
   * @param {{sessionId?: string}} [opts]
   * @returns {Promise<Array<object>>}
   */
  async dispatchMany(tasks, opts = {}) {
    if (!Array.isArray(tasks)) {
      throw new TypeError('Orchestrator: dispatchMany expects an array of tasks');
    }
    if (tasks.length === 0) return [];
    const sequentialRun = this.#executionMode === 'sequential' || (this.#executionMode === 'auto' && tasks.length === 1);
    if (sequentialRun) {
      const results = [];
      for (const task of tasks) {
        results.push(await this.dispatch(task, opts));
      }
      return results;
    }
    const profile = this.#selectProfile(opts.agentId);
    const results = new Array(tasks.length);
    let nextIndex = 0;
    const workerCount = Math.min(this.#maxConcurrentTasks, tasks.length);
    const workers = [];
    for (let w = 0; w < workerCount; w++) {
      workers.push((async () => {
        while (true) {
          const index = nextIndex++;
          if (index >= tasks.length) return;
          if (this.#isBudgetExhausted()) {
            results[index] = this.#budgetDenied(profile.id);
            continue;
          }
          results[index] = await this.#runTask(profile, () => this.#acquireRuntime(profile), tasks[index]);
        }
      })());
    }
    await Promise.all(workers);
    return results;
  }

  /**
   * Run tasks sequentially, feeding each previous output into the next
   * prompt; stop at the first failed task without appending it.
   * @param {string[]} tasks
   * @returns {Promise<{results: Array<object>, finalOutput: *, totalSteps: number, success: boolean, error?: string}>}
   */
  async pipeline(tasks) {
    if (!Array.isArray(tasks)) {
      throw new TypeError('Orchestrator: pipeline expects an array of tasks');
    }
    const results = [];
    let totalSteps = 0;
    let previousOutput = null;
    for (let i = 0; i < tasks.length; i++) {
      const prompt = i === 0 || previousOutput == null
        ? tasks[i]
        : `${tasks[i]}\n\nPrevious step output:\n${previousOutput}`;
      const result = await this.dispatch(prompt);
      if (!result.success) {
        return {
          results,
          finalOutput: results.length > 0 ? results[results.length - 1].output : null,
          totalSteps,
          success: false,
          error: result.error ?? 'orchestrator: task failed',
        };
      }
      results.push(result);
      totalSteps += result.stepsUsed;
      previousOutput = result.output;
    }
    return {
      results,
      finalOutput: results.length > 0 ? results[results.length - 1].output : null,
      totalSteps,
      success: true,
    };
  }

  /**
   * Factual budget snapshot; counters update after every executed TaskResult.
   */
  getBudget() {
    return {
      consumedSteps: this.#budget.consumedSteps,
      dispatchedTasks: this.#budget.dispatchedTasks,
      isStepBudgetExhausted: this.#maxTotalSteps != null && this.#budget.consumedSteps >= this.#maxTotalSteps,
      isTaskBudgetExhausted: this.#maxTotalTasks != null && this.#budget.dispatchedTasks >= this.#maxTotalTasks,
    };
  }

  /**
   * Reset the runner session(s). Idempotent via the cancelled set; program
   * errors from resetSession propagate (never swallowed into false).
   * @param {string} [sessionId] - omit to cancel every recorded session
   * @returns {Promise<boolean>} true when at least one session was removed
   */
  async cancel(sessionId) {
    if (sessionId != null && typeof sessionId !== 'string') {
      throw new TypeError('Orchestrator: sessionId must be a string');
    }
    if (sessionId != null) {
      if (this.#cancelled.has(sessionId)) return false;
      const runtime = this.#sessions.get(sessionId);
      if (!runtime) {
        this.#cancelled.add(sessionId);
        return false;
      }
      const removed = await runtime.resetSession();
      this.#cancelled.add(sessionId);
      if (removed) this.#sessions.delete(sessionId);
      return removed === true;
    }
    let anyRemoved = false;
    for (const [sid, runtime] of [...this.#sessions]) {
      if (this.#cancelled.has(sid)) continue;
      const removed = await runtime.resetSession();
      this.#cancelled.add(sid);
      if (removed) {
        this.#sessions.delete(sid);
        anyRemoved = true;
      }
    }
    return anyRemoved;
  }

  /**
   * Close every live runtime and clear its registrations, continuing after individual failures.
   */
  close() {
    for (const runtime of this.#liveRuntimes) {
      try {
        if (typeof runtime.close === 'function') runtime.close();
      } catch {
      }
    }
    this.#liveRuntimes.clear();
    this.#sessions.clear();
    this.#profileRuntimes.clear();
  }

  /** @returns {object} the first registered profile (throws before any task runs) */
  #selectProfile(agentId) {
    if (this.#profiles.length === 0) {
      throw new Error('Orchestrator: no agent registered — call registerAgent() before dispatching tasks');
    }
    if (agentId != null) {
      const selected = this.#profiles.find(profile => profile.id === agentId);
      if (!selected) throw new Error(`Orchestrator: unknown agent '${agentId}'`);
      return selected;
    }
    return this.#profiles[0];
  }

  /** Acquire runtimes for every registered profile once (pool activation). */
  #warmPool() {
    for (let i = 0; i < this.#profiles.length; i++) {
      this.#ensureProfileRuntime(i);
    }
  }

  /**
   * Memoized per-profile acquisition: exactly one factory call + connect per
   * profile for the lifetime of the pool.
   * @returns {Promise<ClientCoreRuntime>}
   */
  #ensureProfileRuntime(index) {
    let pending = this.#profileRuntimes.get(index);
    if (!pending) {
      pending = this.#acquireRuntime(this.#profiles[index]);
      pending.catch(() => { this.#profileRuntimes.delete(index); });
      this.#profileRuntimes.set(index, pending);
    }
    return pending;
  }

  /**
   * One factory call + connect; tracks the instance for close().
   * @returns {Promise<ClientCoreRuntime>}
   */
  async #acquireRuntime(profile) {
    const runtime = await this.#runtimeFactory({
      serverUrl: this.#serverUrl,
      systemPrompt: profile.systemPrompt,
      maxSteps: profile.maxSteps,
    });
    if (runtime && typeof runtime.connect === 'function') {
      await runtime.connect();
    }
    this.#liveRuntimes.add(runtime);
    return runtime;
  }

  /**
   * Execute one turn and fold it into a TaskResult; budget counters update
   * synchronously right after the result exists (single-threaded event loop
   * makes that atomic). Runtime failures become failed TaskResults, never
   * rejections — callers branch on success instead of try/catch.
   */
  async #runTask(profile, runtimeSource, prompt) {
    if (this.#isBudgetExhausted()) return this.#budgetDenied(profile.id);
    const reservation = Math.min(profile.maxSteps ?? 10, this.#maxTotalSteps == null ? Infinity : this.#maxTotalSteps - this.#budget.consumedSteps - this.#reservedSteps);
    if (reservation <= 0) return this.#budgetDenied(profile.id);
    this.#reservedSteps += reservation;
    this.#inFlightTasks += 1;
    const start = Date.now();
    const taskId = `task-${++this.#taskSeq}`;
    try {
      const runtime = await (typeof runtimeSource === 'function' ? runtimeSource() : runtimeSource);
      if (typeof runtime.connect === 'function') await runtime.connect();
      if (runtime.sessionId) this.#sessions.set(runtime.sessionId, runtime);
      const turn = await runtime.runTurn(prompt, { maxSteps: reservation });
      if (turn && typeof turn.sessionId === 'string' && turn.sessionId !== '') {
        this.#sessions.set(turn.sessionId, runtime);
      }
      const result = {
        taskId,
        agentId: profile.id,
        output: turn.content ?? null,
        success: turn.action === 'final',
        stepsUsed: turn.iterations ?? 0,
        sessionId: turn.sessionId ?? '',
        durationMs: Date.now() - start,
      };
      this.#budget.consumedSteps += result.stepsUsed;
      this.#budget.dispatchedTasks += 1;
      return result;
    } catch (err) {
      this.#budget.consumedSteps += err?.stepsUsed ?? 0;
      this.#budget.dispatchedTasks += 1;
      return {
        taskId,
        agentId: profile.id,
        output: null,
        success: false,
        stepsUsed: err?.stepsUsed ?? 0,
        sessionId: err?.sessionId ?? '',
        error: err instanceof Error ? err.message : String(err),
        durationMs: Date.now() - start,
      };
    } finally {
      this.#reservedSteps -= reservation;
      this.#inFlightTasks -= 1;
    }
  }

  /** Budget-denied result: no runTurn call, no counter update (nothing ran). */
  #isBudgetExhausted() {
    const budget = this.getBudget();
    return budget.isStepBudgetExhausted || budget.isTaskBudgetExhausted || (this.#maxTotalTasks != null && this.#budget.dispatchedTasks + this.#inFlightTasks >= this.#maxTotalTasks) || (this.#maxTotalSteps != null && this.#budget.consumedSteps + this.#reservedSteps >= this.#maxTotalSteps);
  }

  #budgetDenied(agentId) {
    return {
      taskId: `task-${++this.#taskSeq}`,
      agentId,
      output: null,
      success: false,
      stepsUsed: 0,
      sessionId: '',
      error: 'orchestrator: task budget exhausted',
      errorKind: 'permanent',
      durationMs: 0,
    };
  }
}

module.exports = { Orchestrator };
