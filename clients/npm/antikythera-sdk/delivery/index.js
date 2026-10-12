"use strict";
/** Configures server execution and exports the same agent as a standalone WASM/jco client bundle. */
const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const crypto = require("node:crypto");
const http = require("node:http");
const { pathToFileURL } = require("node:url");
const { createUnionRegistry } = require("../runtime/registry.js");
const { extractToolEntry, isTool } = require("../runtime/tools.js");
const { version } = require("../package.json");

/** Accepts only portable relative module paths; every dependency must live under the binding directory. */
function modulePath(value) {
  if (
    typeof value !== "string" ||
    !/\.(?:js|mjs)$/.test(value) ||
    value.includes("\\") ||
    value.includes(":") ||
    value.split("/").some((part) => !part || part === "." || part === "..") ||
    path.isAbsolute(value)
  ) {
    throw new Error(
      "client binding must be a relative .js or .mjs path inside bindingsDir",
    );
  }
  return value;
}

/** Copies regular files and directories without following links outside the requested binding environment. */
function copyDirectory(source, destination) {
  fs.mkdirSync(destination, { recursive: true });
  for (const entry of fs.readdirSync(source, { withFileTypes: true })) {
    const from = path.join(source, entry.name);
    const to = path.join(destination, entry.name);
    if (entry.isDirectory()) copyDirectory(from, to);
    else if (entry.isFile()) fs.copyFileSync(from, to);
    else
      throw new Error(
        `delivery bundles cannot include symbolic links: ${from}`,
      );
  }
}

/** Builds a deterministic inventory for independent integrity verification of the delivered environment. */
function fileInventory(directory, relative = "") {
  return fs
    .readdirSync(path.join(directory, relative), { withFileTypes: true })
    .sort((a, b) => a.name.localeCompare(b.name, "en"))
    .flatMap((entry) => {
      const name = [relative, entry.name].filter(Boolean).join("/");
      if (entry.isDirectory()) return fileInventory(directory, name);
      const bytes = fs.readFileSync(path.join(directory, name));
      return [
        {
          path: name,
          size: bytes.length,
          sha256: crypto.createHash("sha256").update(bytes).digest("hex"),
        },
      ];
    });
}

/** Retries temporary Windows sharing failures while preserving the staged-directory atomic install. */
async function installExport(source, target) {
  for (let attempt = 0; ; attempt++) {
    try {
      fs.renameSync(source, target);
      return;
    } catch (error) {
      if (
        process.platform !== "win32" ||
        !["EPERM", "EACCES", "EBUSY"].includes(error.code) ||
        attempt >= 5
      )
        throw error;
      await new Promise((resolve) => setTimeout(resolve, 40 * 2 ** attempt));
    }
  }
}

class AgentDeployment {
  /** Keeps server callbacks separate from the whitelisted configuration exported to clients. */
  constructor(config, environment = {}) {
    const allowed = new Set([
      "id",
      "provider",
      "model",
      "systemPrompt",
      "maxSteps",
      "timeoutMs",
      "timeout",
    ]);
    for (const key of Object.keys(config ?? {}))
      if (!allowed.has(key))
        throw new Error(
          `unknown agent configuration field '${key}'; put host capabilities in environment`,
        );
    this.environment = { ...environment };
    this.tools = (environment.tools ?? []).map((raw) => {
      const value = raw?.tool ?? raw;
      const entry = isTool(value) ? extractToolEntry(value) : raw;
      if (typeof entry?.handler !== "function")
        throw new Error("deployment tools require server handlers");
      if (entry.owner && entry.owner !== "server")
        throw new Error("deployment tools require server-owned handlers");
      if (raw.allow !== undefined && typeof raw.allow !== "boolean")
        throw new Error("tool.allow must be a boolean");
      return {
        definition: structuredClone(entry.definition),
        handler: entry.handler,
        allow: raw.allow === true,
      };
    });
    const registry = createUnionRegistry();
    const names = new Set();
    for (const tool of this.tools) {
      if (names.has(tool.definition.name))
        throw new Error(`duplicate deployment tool '${tool.definition.name}'`);
      names.add(tool.definition.name);
      registry.add(tool.definition, "server");
      tool.definition = registry
        .toDefinitions()
        .find((entry) => entry.name === tool.definition.name);
    }
    this._definition = {
      schemaVersion: 1,
      sdkVersion: version,
      serverUrl: null,
      agent: {
        id: config?.id,
        provider: config?.provider,
        model: config?.model,
        systemPrompt: config?.systemPrompt ?? null,
        maxSteps: config?.maxSteps ?? 8,
        timeoutMs: config?.timeoutMs ?? config?.timeout ?? 60000,
        hooksModule: null,
        tools: this.tools.map((tool) => ({
          definition: tool.definition,
          allow: tool.allow,
          execution: "server",
          module: null,
        })),
      },
    };
    for (const field of ["id", "provider", "model"])
      if (
        typeof this.definition.agent[field] !== "string" ||
        !this.definition.agent[field].trim()
      )
        throw new Error(`agent.${field} must be a non-empty string`);
    if (
      this.definition.agent.systemPrompt !== null &&
      typeof this.definition.agent.systemPrompt !== "string"
    )
      throw new Error("systemPrompt must be a string or null");
    for (const field of ["maxSteps", "timeoutMs"])
      if (
        !Number.isSafeInteger(this.definition.agent[field]) ||
        this.definition.agent[field] < 0
      )
        throw new Error(`agent.${field} must be a non-negative integer`);
    this.ready = import("./runtime.mjs").then(({ validateDefinition }) => {
      this._definition = validateDefinition(this._definition);
    });
    this.runtime = null;
    this.turns = Promise.resolve();
  }

  /** Returns a detached public configuration so callers cannot mutate the deployment snapshot. */
  get definition() {
    return structuredClone(this._definition);
  }

  /** Pins provider/model selection to this deployment instead of accepting caller-selected credentials or providers. */
  async callProvider(request, options = {}) {
    await this.ready;
    if (
      typeof request.messages_json !== "string" ||
      !Array.isArray(JSON.parse(request.messages_json))
    )
      throw new TypeError("messages_json must encode an array");
    if (typeof this.environment.provider !== "function")
      throw new Error("server execution requires environment.provider");
    options.signal?.throwIfAborted();
    const work = this.environment.provider(
      {
        ...request,
        provider: this.definition.agent.provider,
        model: this.definition.agent.model,
      },
      options,
    );
    const response = await (options.signal
      ? (await import("./runtime.mjs")).abortable(work, options.signal)
      : work);
    if (typeof response?.content !== "string")
      throw new Error("provider response.content must be a string");
    return response;
  }

  /** Enforces registration and permission before invoking a server-owned tool. */
  async executeTool(call, options = {}) {
    const tool = this.tools.find(
      (entry) => entry.definition.name === call["tool-name"],
    );
    if (!tool?.allow)
      throw new Error(`permission: tool '${call["tool-name"]}' is not allowed`);
    const input = JSON.parse(call["arguments-json"] ?? "{}");
    let output;
    let error = null;
    try {
      options.signal?.throwIfAborted();
      const work = tool.handler(input, options);
      output = JSON.stringify(
        (await (options.signal
          ? (await import("./runtime.mjs")).abortable(work, options.signal)
          : work)) ?? null,
      );
      if (output === undefined)
        throw new Error("tool output must be JSON serializable");
    } catch (failure) {
      output = "{}";
      error = String(failure.message ?? failure);
    }
    return {
      "tool-name": tool.definition.name,
      success: error === null,
      "output-json": output,
      "error-message": error,
      "step-id": call["step-id"] ?? 0,
    };
  }

  /** Creates an independent server-side WASM instance with this deployment's host capabilities. */
  async createRuntime() {
    await this.ready;
    const { createConfiguredAgent } = await import("./runtime.mjs");
    return createConfiguredAgent(this.definition, {
      componentUrl: pathToFileURL(
        path.join(__dirname, "../component/instantiate.js"),
      ).href,
      hooks: this.environment.hooks,
      provider: (request, options) => this.callProvider(request, options),
      executeTool: (call, options) => this.executeTool(call, options),
    });
  }

  /** Serializes consecutive turns in an instance-owned server session and reports execution failures as results. */
  async run(prompt, options = {}) {
    if (typeof prompt !== "string" || !prompt.trim())
      throw new TypeError("prompt must be a non-empty string");
    const task = this.turns.then(async () => {
      if (!this.runtime)
        this.runtime = this.createRuntime().catch((error) => {
          this.runtime = null;
          throw error;
        });
      const runtime = await this.runtime;
      try {
        return await runtime.run(prompt, options);
      } catch (error) {
        return {
          output: "",
          success: false,
          stepsUsed: 0,
          sessionId: runtime.sessionId,
          error: String(error.message ?? error),
        };
      }
    });
    this.turns = task.then(
      () => {},
      () => {},
    );
    return task;
  }

  /** Releases the deployment-owned session after already accepted turns have completed. */
  async close() {
    await this.turns;
    if (this.runtime) (await this.runtime).close();
    this.runtime = null;
  }

  /** Exports config, the raw Component Model artifact, jco factory, host adapters, and explicit client bindings. */
  async exportClient(destination, options = {}) {
    await this.ready;
    const definition = structuredClone(this.definition);
    definition.serverUrl = options.serverUrl ?? null;
    const mapping = options.clientTools ?? {};
    for (const name of Object.keys(mapping))
      if (!this.tools.some((tool) => tool.definition.name === name))
        throw new Error(`unknown client tool binding '${name}'`);
    definition.agent.tools = definition.agent.tools.map((tool) =>
      mapping[tool.definition.name]
        ? {
            ...tool,
            execution: "client",
            module: modulePath(mapping[tool.definition.name]),
          }
        : tool,
    );
    definition.agent.hooksModule = options.hooksModule
      ? modulePath(options.hooksModule)
      : null;
    if (
      ["prepareTurn", "decideAction", "handleToolResult"].some(
        (name) => typeof this.environment.hooks?.[name] === "function",
      ) &&
      !definition.agent.hooksModule
    )
      throw new Error(
        "configured server hooks require a hooksModule client binding",
      );
    const { validateDefinition } = await import("./runtime.mjs");
    validateDefinition(definition);
    const bindings = [
      ...Object.values(mapping),
      definition.agent.hooksModule,
    ].filter(Boolean);
    if (bindings.length && !options.bindingsDir)
      throw new Error("bindingsDir is required for client modules");
    for (const binding of bindings) {
      if (
        !fs
          .statSync(path.join(options.bindingsDir, modulePath(binding)))
          .isFile()
      )
        throw new Error(`client binding missing: ${binding}`);
    }
    const componentBytes = fs.readFileSync(path.join(__dirname, "agent.wasm"));
    if (
      !componentBytes
        .subarray(0, 8)
        .equals(Buffer.from([0, 97, 115, 109, 13, 0, 1, 0]))
    )
      throw new Error(
        "delivery artifact must be a WASM Component Model binary; run task transpile",
      );
    const target = path.resolve(destination);
    if (fs.existsSync(target))
      throw new Error("export destination already exists; use a new directory");
    fs.mkdirSync(path.dirname(target), { recursive: true });
    const stage = fs.mkdtempSync(
      path.join(path.dirname(target), ".agent-export-"),
    );
    try {
      copyDirectory(
        path.join(__dirname, "../component"),
        path.join(stage, "component"),
      );
      if (options.bindingsDir)
        copyDirectory(
          path.resolve(options.bindingsDir),
          path.join(stage, "bindings"),
        );
      fs.writeFileSync(path.join(stage, "agent.wasm"), componentBytes);
      fs.copyFileSync(
        path.join(__dirname, "runtime.mjs"),
        path.join(stage, "runtime.mjs"),
      );
      fs.copyFileSync(
        path.join(__dirname, "client.d.ts"),
        path.join(stage, "agent.d.ts"),
      );
      fs.writeFileSync(
        path.join(stage, "package.json"),
        JSON.stringify({ type: "module", private: true }),
      );
      fs.writeFileSync(
        path.join(stage, "agent.json"),
        JSON.stringify(definition, null, 2),
      );
      fs.writeFileSync(
        path.join(stage, "agent.js"),
        `/** Creates this configured agent with a private jco component instance. */\nimport { createConfiguredAgent } from './runtime.mjs';\nconst definition = ${JSON.stringify(definition)};\nexport { definition };\nexport function createAgent(environment = {}) {\n  return createConfiguredAgent(definition, { ...environment, baseUrl: import.meta.url, componentUrl: new URL('./component/instantiate.js', import.meta.url).href });\n}\n`,
      );
      const manifest = {
        schemaVersion: 1,
        sdkVersion: version,
        agentId: definition.agent.id,
        entry: "agent.js",
        configuration: "agent.json",
        component: "agent.wasm",
        jco: "component/instantiate.js",
        files: fileInventory(stage),
      };
      fs.writeFileSync(
        path.join(stage, "manifest.json"),
        JSON.stringify(manifest, null, 2),
      );
      await installExport(stage, target);
      return manifest;
    } finally {
      if (fs.existsSync(stage))
        fs.rmSync(stage, { recursive: true, force: true });
    }
  }

  /** Starts a small HTTP host for server execution, provider/tool proxies, and the configured client bundle. */
  async serve(options = {}) {
    await this.ready;
    const bind = options.bind ?? "127.0.0.1";
    if (!["127.0.0.1", "localhost", "::1"].includes(bind) && !options.authToken)
      throw new Error("non-loopback delivery hosts require authToken");
    const temporary = fs.mkdtempSync(
      path.join(os.tmpdir(), "antikythera-delivery-"),
    );
    const bundle = path.join(temporary, "bundle");
    const server = http.createServer();
    let url;
    try {
      await new Promise((resolve, reject) => {
        server.once("error", reject);
        server.listen(options.port ?? 0, bind, resolve);
      });
      const address = server.address();
      url = `http://${bind.includes(":") ? `[${bind}]` : bind}:${address.port}`;
      await this.exportClient(bundle, {
        ...options,
        serverUrl: options.publicUrl ?? url,
      });
    } catch (error) {
      server.close();
      fs.rmSync(temporary, { recursive: true, force: true });
      throw error;
    }
    server.on("request", async (req, res) => {
      const send = (status, value) => {
        res.writeHead(status, { "Content-Type": "application/json" });
        res.end(JSON.stringify(value));
      };
      const origin = req.headers.origin;
      if (
        origin &&
        origin !== new URL(url).origin &&
        origin !== new URL(options.publicUrl ?? url).origin &&
        !(options.allowedOrigins ?? []).includes(origin)
      ) {
        send(403, { error: "origin is not allowed" });
        return;
      }
      if (origin) {
        res.setHeader("Access-Control-Allow-Origin", origin);
        res.setHeader("Vary", "Origin");
      }
      if (req.method === "OPTIONS") {
        res.writeHead(204, {
          "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
          "Access-Control-Allow-Headers": "Content-Type, Authorization",
        });
        res.end();
        return;
      }
      const controller = new AbortController();
      req.on("aborted", () => controller.abort(new Error("request aborted")));
      const timer = setTimeout(
        () => controller.abort(new Error("host request timed out")),
        this.definition.agent.timeoutMs,
      );
      try {
        const pathname = decodeURIComponent(new URL(req.url, url).pathname);
        if (req.method === "GET" && pathname.startsWith("/bundle/")) {
          const relative = pathname.slice("/bundle/".length);
          const target = path.resolve(bundle, relative);
          if (
            !target.startsWith(`${bundle}${path.sep}`) ||
            !fs.existsSync(target) ||
            !fs.statSync(target).isFile()
          ) {
            send(404, { error: "bundle file not found" });
            return;
          }
          const mime =
            {
              ".wasm": "application/wasm",
              ".js": "text/javascript",
              ".mjs": "text/javascript",
              ".json": "application/json",
              ".html": "text/html",
              ".css": "text/css",
              ".svg": "image/svg+xml",
              ".png": "image/png",
              ".jpg": "image/jpeg",
              ".webp": "image/webp",
              ".txt": "text/plain",
            }[path.extname(target)] ?? "application/octet-stream";
          res.writeHead(200, {
            "Content-Type": mime,
            "Cache-Control": "no-store",
          });
          res.end(fs.readFileSync(target));
          return;
        }
        if (
          req.method !== "POST" ||
          !["/run", "/llm/call", "/tools/execute"].includes(pathname)
        ) {
          send(404, { error: "route not found" });
          return;
        }
        if (
          options.authToken &&
          req.headers.authorization !== `Bearer ${options.authToken}`
        ) {
          send(401, { error: "authentication required" });
          return;
        }
        const chunks = [];
        let size = 0;
        for await (const chunk of req) {
          size += chunk.length;
          if (size > 1024 * 1024) {
            send(413, { error: "request exceeds 1 MiB" });
            return;
          }
          chunks.push(chunk);
        }
        const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
        if (!body || typeof body !== "object" || Array.isArray(body))
          throw new Error("request must be an object");
        let result;
        if (pathname === "/run") {
          if (typeof body.prompt !== "string" || !body.prompt.trim())
            throw new TypeError("prompt must be a non-empty string");
          const runtime = await this.createRuntime();
          try {
            result = await runtime.run(body.prompt, {
              signal: controller.signal,
            });
          } catch (error) {
            result = {
              output: "",
              success: false,
              stepsUsed: 0,
              sessionId: runtime.sessionId,
              error: String(error.message ?? error),
            };
          } finally {
            runtime.close();
          }
        } else if (pathname === "/llm/call")
          result = await this.callProvider(body, { signal: controller.signal });
        else
          result = await this.executeTool(body, { signal: controller.signal });
        send(200, result);
      } catch (error) {
        send(
          String(error.message).startsWith("permission:")
            ? 403
            : error instanceof SyntaxError || error instanceof TypeError
              ? 400
              : 500,
          { error: String(error.message ?? error) },
        );
      } finally {
        clearTimeout(timer);
      }
    });
    return {
      url,
      clientUrl: `${options.publicUrl ?? url}/bundle/agent.js`,
      manifestUrl: `${options.publicUrl ?? url}/bundle/manifest.json`,
      /** Stops the host and removes only its own temporary export directory. */
      close: async () => {
        await new Promise((resolve) => {
          server.close(resolve);
          server.closeAllConnections?.();
        });
        fs.rmSync(temporary, { recursive: true, force: true });
      },
    };
  }
}

/** Creates a deployment whose server environment is never serialized into client configuration. */
function createAgentDeployment(config, environment = {}) {
  return new AgentDeployment(config, environment);
}

module.exports = { AgentDeployment, createAgentDeployment };
