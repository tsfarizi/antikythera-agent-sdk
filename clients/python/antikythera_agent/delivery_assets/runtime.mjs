/** Executes a configured agent through a private jco instance in Node.js or a browser. */

/** Validates the public configuration shared by Python and TypeScript deployment exports. */
export function validateDefinition(definition) {
  if (definition?.schemaVersion !== 1)
    throw new Error("unsupported agent delivery schema");
  const agent = definition.agent;
  for (const name of ["id", "provider", "model"]) {
    if (typeof agent?.[name] !== "string" || !agent[name].trim())
      throw new Error(`agent.${name} must be a non-empty string`);
  }
  if (agent.systemPrompt !== null && typeof agent.systemPrompt !== "string")
    throw new Error("agent.systemPrompt must be a string or null");
  for (const name of ["maxSteps", "timeoutMs"]) {
    if (!Number.isSafeInteger(agent[name]) || agent[name] < 0)
      throw new Error(`agent.${name} must be a non-negative integer`);
  }
  if (!Array.isArray(agent.tools))
    throw new Error("agent.tools must be an array");
  const names = new Set();
  for (const tool of agent.tools) {
    const name = tool.definition?.name;
    if (typeof name !== "string" || !name || names.has(name))
      throw new Error("tool names must be non-empty and unique");
    if (
      typeof tool.definition.description !== "string" ||
      !tool.definition.description
    )
      throw new Error(`tool '${name}' requires a description`);
    if (
      typeof tool.allow !== "boolean" ||
      !["server", "client"].includes(tool.execution)
    )
      throw new Error(`invalid tool binding '${name}'`);
    names.add(name);
  }
  if (definition.serverUrl !== null) {
    const url = new URL(definition.serverUrl);
    if (
      !["http:", "https:"].includes(url.protocol) ||
      url.username ||
      url.password ||
      url.search ||
      url.hash
    )
      throw new Error(
        "serverUrl must be an HTTP base URL without credentials, query, or fragment",
      );
  }
  return structuredClone(definition);
}

/** Races host work against cancellation without committing responses after a cancelled turn. */
export function abortable(work, signal) {
  signal.throwIfAborted();
  return new Promise((resolve, reject) => {
    const abort = () =>
      reject(signal.reason ?? new Error("agent turn cancelled"));
    signal.addEventListener("abort", abort, { once: true });
    Promise.resolve(work)
      .then(resolve, reject)
      .finally(() => signal.removeEventListener("abort", abort));
  });
}

/** Creates an instance-owned tool loop; host callbacks and credentials remain outside the manifest. */
export async function createConfiguredAgent(input, environment = {}) {
  const definition = validateDefinition(input);
  const config = definition.agent;
  const serverUrl = environment.serverUrl ?? definition.serverUrl;
  const bindings = { ...environment.tools };
  for (const tool of config.tools) {
    if (tool.execution === "client" && !bindings[tool.definition.name]) {
      if (!tool.module || !environment.baseUrl)
        throw new Error(
          `missing client binding for tool '${tool.definition.name}'`,
        );
      const module = await import(
        new URL(`./bindings/${tool.module}`, environment.baseUrl).href
      );
      bindings[tool.definition.name] = module.default ?? module.handler;
    }
    if (
      tool.execution === "client" &&
      typeof bindings[tool.definition.name] !== "function"
    )
      throw new Error(
        `client tool '${tool.definition.name}' must export a handler`,
      );
  }
  let hooks = environment.hooks;
  if (config.hooksModule && !hooks) {
    const module = await import(
      new URL(`./bindings/${config.hooksModule}`, environment.baseUrl).href
    );
    hooks = module.default ?? module.hooks;
    if (!hooks || typeof hooks !== "object")
      throw new Error("hooks module must export a hooks object");
  }
  if (!environment.componentUrl)
    throw new Error("componentUrl is required for jco instantiation");
  const runner = await (
    await import(environment.componentUrl)
  ).createRunner(hooks);
  const entropy = globalThis.crypto ?? (await import("node:crypto")).webcrypto;
  const sessionKey = Array.from(
    entropy.getRandomValues(new Uint8Array(16)),
    (byte) => byte.toString(16).padStart(2, "0"),
  ).join("");
  const sessionId = runner.init(
    JSON.stringify({
      max_steps: config.maxSteps,
      session_id: `delivery-${sessionKey}`,
      auto_execute_tools: false,
      runtime_hooks_enabled: !!hooks,
    }),
  );
  runner.registerTools(
    JSON.stringify(config.tools.map((tool) => tool.definition)),
  );
  let active = null;
  let closed = false;

  /** Sends only runtime requests to the configured host; tokens are supplied by the caller. */
  async function post(route, body, signal) {
    if (!serverUrl)
      throw new Error(`serverUrl is required for remote ${route}`);
    const headers = { "Content-Type": "application/json" };
    if (environment.authToken)
      headers.Authorization = `Bearer ${environment.authToken}`;
    const response = await fetch(`${serverUrl.replace(/\/$/, "")}${route}`, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
      signal,
    });
    const value = await response.json();
    if (!response.ok)
      throw new Error(value.error ?? `agent host HTTP ${response.status}`);
    return value;
  }

  /** Runs the same prepare/commit/tool-result protocol used by the native Python component host. */
  async function perform(prompt, signal) {
    const events = [];
    for (let step = 0; step < config.maxSteps; step++) {
      signal.throwIfAborted();
      const correlationId = `delivery-${step + 1}`;
      const preparedJson = runner.prepareUserTurn(
        JSON.stringify({
          prompt: step === 0 ? prompt : "[continue]",
          session_id: sessionId,
          system_prompt: config.systemPrompt,
          correlation_id: correlationId,
        }),
      );
      const prepared = JSON.parse(preparedJson);
      const request = {
        provider: config.provider,
        model: config.model,
        session_id: sessionId,
        messages_json: prepared.messages_json,
        force_json: false,
        temperature: null,
        max_tokens: null,
        schema_name: null,
        metadata_json: null,
      };
      const response = await abortable(
        environment.provider
          ? environment.provider(request, { signal })
          : post("/llm/call", request, signal),
        signal,
      );
      if (typeof response?.content !== "string")
        throw new Error("provider response.content must be a string");
      signal.throwIfAborted();
      const commit = JSON.parse(
        runner.commitLlmResponse(preparedJson, response.content),
      );
      const drained = JSON.parse(runner.drainEvents(sessionId));
      events.push(...drained);
      if (commit.action === "final")
        return {
          output: commit.content ?? "",
          success: true,
          stepsUsed: step + 1,
          sessionId,
          events,
        };
      if (commit.action !== "call_tool")
        throw new Error(
          `runner action '${commit.action}': ${commit.content ?? ""}`,
        );
      const name = commit.tool_name;
      if (
        drained.some(
          (event) =>
            event.kind === "tool_result" && event.payload?.tool === name,
        )
      )
        continue;
      const tool = config.tools.find((entry) => entry.definition.name === name);
      if (!tool?.allow)
        throw new Error(`permission: tool '${name}' is not allowed`);
      let result;
      if (tool.execution === "client") {
        try {
          const output = await abortable(
            bindings[name](commit.tool_input ?? {}, { signal }),
            signal,
          );
          const outputJson = JSON.stringify(output ?? null);
          if (outputJson === undefined)
            throw new Error("tool output must be JSON serializable");
          result = {
            success: true,
            "output-json": outputJson,
            "error-message": null,
          };
        } catch (error) {
          signal.throwIfAborted();
          result = {
            success: false,
            "output-json": "{}",
            "error-message": String(error.message ?? error),
          };
        }
      } else {
        const call = {
          "tool-name": name,
          "arguments-json": JSON.stringify(commit.tool_input ?? {}),
          "session-id": sessionId,
          "step-id": commit.step ?? 0,
        };
        result = await abortable(
          environment.executeTool
            ? environment.executeTool(call, { signal })
            : post("/tools/execute", call, signal),
          signal,
        );
      }
      signal.throwIfAborted();
      if (
        typeof result?.success !== "boolean" ||
        typeof result["output-json"] !== "string"
      )
        throw new Error("invalid tool execution response");
      runner.processToolResultForSession(
        sessionId,
        JSON.stringify({
          tool_name: name,
          success: result.success,
          output_json: result["output-json"],
          error_message: result["error-message"] ?? null,
          correlation_id: correlationId,
        }),
      );
    }
    throw new Error(
      `agent exceeded max_steps (${config.maxSteps}) without a final response`,
    );
  }

  return {
    sessionId,
    getState: () => JSON.parse(runner.getState(sessionId)),
    /** Rejects overlapping turns and bounds host work by the agent's configured timeout. */
    async run(prompt, options = {}) {
      if (closed) throw new Error("agent runtime is closed");
      if (active) throw new Error("agent runtime already has an active turn");
      if (typeof prompt !== "string" || !prompt.trim())
        throw new Error("prompt must be a non-empty string");
      const controller = new AbortController();
      active = controller;
      const abort = () => controller.abort(options.signal.reason);
      if (options.signal?.aborted) abort();
      else options.signal?.addEventListener("abort", abort, { once: true });
      const timer = setTimeout(
        () => controller.abort(new Error("agent turn timed out")),
        config.timeoutMs,
      );
      try {
        return await perform(prompt, controller.signal);
      } finally {
        clearTimeout(timer);
        options.signal?.removeEventListener("abort", abort);
        active = null;
        if (closed) runner.resetSession(sessionId);
      }
    },
    /** Releases session state and prevents further turns on this instance. */
    close() {
      closed = true;
      active?.abort(new Error("agent runtime closed"));
      if (!active) runner.resetSession(sessionId);
    },
  };
}
