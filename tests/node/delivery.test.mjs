/** Acceptance tests for server execution and portable exports using real jco Component Model instances. */
import assert from "node:assert/strict";
import { test } from "node:test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import crypto from "node:crypto";
import { pathToFileURL, fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import delivery from "../../clients/npm/antikythera-sdk/delivery/index.js";
import { defineTool } from "../../clients/npm/antikythera-sdk/runtime/tools.js";

test("private delivery sessions remain distinct when component clocks have the same millisecond", async () => {
  const realNow = Date.now;
  const runtimes = [];
  Date.now = () => 1700000000000;
  try {
    const deployment = delivery.createAgentDeployment(config, environment());
    runtimes.push(
      ...(await Promise.all(
        Array.from({ length: 4 }, () => deployment.createRuntime()),
      )),
    );
    assert.equal(new Set(runtimes.map((runtime) => runtime.sessionId)).size, 4);
  } finally {
    Date.now = realNow;
    for (const runtime of runtimes) runtime.close();
  }
});

const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const config = {
  id: "receipt-agent",
  provider: "receipt",
  model: "configured-model",
  systemPrompt: "Shared agent prompt",
  maxSteps: 4,
};

test("reuses configured ToolRef handlers and keeps deployment metadata detached", async () => {
  let calls = 0;
  const receipt = defineTool(
    { name: "delivery_receipt", description: "Return a numeric receipt" },
    (args) => {
      calls++;
      return { receipt: args.value };
    },
  );
  const deployment = delivery.createAgentDeployment(
    { ...config, timeout: 5000 },
    {
      provider: environment().provider,
      tools: [{ tool: receipt, allow: true }],
    },
  );
  const detached = deployment.definition;
  detached.agent.model = "changed";
  assert.equal(deployment.definition.agent.model, config.model);
  assert.equal(deployment.definition.agent.timeoutMs, 5000);
  try {
    assert.equal((await deployment.run("hello")).output, "delivered-7");
    assert.equal(calls, 1);
  } finally {
    await deployment.close();
  }
});

test("convenience execution reports policy failures without executing denied handlers", async () => {
  const env = environment();
  env.tools[0].allow = false;
  env.tools[0].handler = () => {
    throw new Error("denied handler executed");
  };
  const deployment = delivery.createAgentDeployment(config, env);
  try {
    const result = await deployment.run("hello");
    assert.equal(result.success, false);
    assert.match(result.error, /permission:/);
    await assert.rejects(deployment.run(""), /prompt/);
  } finally {
    await deployment.close();
  }
});

function directory(t) {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "agent-delivery-test-"));
  t.after(() => fs.rmSync(temp, { recursive: true, force: true }));
  return temp;
}

/** Uses runner-owned tool history to drive a real two-step tool loop. */
function environment(extra = {}) {
  return {
    provider: async (request) => ({
      content: JSON.parse(request.messages_json).some(
        (message) => message.role === "tool",
      )
        ? '{"action":"final","content":"delivered-7"}'
        : '{"action":"call_tool","tool":"delivery_receipt","input":{"value":7}}',
    }),
    tools: [
      {
        definition: {
          name: "delivery_receipt",
          description: "Return a receipt for the supplied numeric value",
        },
        handler: (args) => ({ receipt: args.value }),
        allow: true,
      },
    ],
    ...extra,
  };
}

function verifyInventory(bundle, manifest) {
  for (const file of manifest.files) {
    const bytes = fs.readFileSync(path.join(bundle, file.path));
    assert.equal(bytes.length, file.size);
    assert.equal(
      crypto.createHash("sha256").update(bytes).digest("hex"),
      file.sha256,
    );
  }
  assert.ok(manifest.files.some((file) => file.path === "agent.wasm"));
  assert.ok(
    manifest.files.some((file) =>
      file.path.includes("component-abi-adapters.core.wasm"),
    ),
  );
  assert.ok(
    fs
      .readFileSync(path.join(bundle, "agent.wasm"))
      .equals(
        fs.readFileSync(
          path.join(root, "clients/python/antikythera_agent/antikythera.wasm"),
        ),
      ),
  );
}

test("Node runs configured agents with private jco sessions and preserved provider/model/prompt", async () => {
  const requests = [];
  const env = environment();
  const deployment = delivery.createAgentDeployment(config, {
    ...env,
    provider: async (request) => {
      requests.push(request);
      return env.provider(request);
    },
  });
  const a = await deployment.createRuntime();
  const b = await deployment.createRuntime();
  try {
    const result = await a.run("hello");
    assert.equal(result.output, "delivered-7");
    assert.equal(result.stepsUsed, 2);
    assert.equal(result.sessionId, a.sessionId);
    assert.notEqual(a.sessionId, b.sessionId);
    assert.equal(b.getState().message_history.length, 0);
    assert.ok(
      requests.every(
        (request) =>
          request.provider === config.provider &&
          request.model === config.model,
      ),
    );
    assert.match(requests[0].messages_json, /Shared agent prompt/);
    assert.match(requests[1].messages_json, /receipt/);
  } finally {
    a.close();
    b.close();
  }
});

test("exports a standalone client environment with verified artifacts and an offline JS tool binding", async (t) => {
  const temp = directory(t);
  const bindings = path.join(temp, "bindings");
  fs.mkdirSync(bindings);
  fs.writeFileSync(
    path.join(bindings, "echo.mjs"),
    "/** Return a portable receipt. */\nexport default args => ({receipt: args.value});",
  );
  let serverCalls = 0;
  const env = environment();
  env.tools[0].handler = () => {
    serverCalls++;
    throw new Error("must run on client");
  };
  const deployment = delivery.createAgentDeployment(config, env);
  const bundle = path.join(temp, "export");
  const manifest = await deployment.exportClient(bundle, {
    bindingsDir: bindings,
    clientTools: { delivery_receipt: "echo.mjs" },
  });
  verifyInventory(bundle, manifest);
  const { createAgent, definition } = await import(
    pathToFileURL(path.join(bundle, "agent.js")).href
  );
  assert.equal(definition.agent.tools[0].execution, "client");
  const runtime = await createAgent({ provider: environment().provider });
  try {
    assert.equal((await runtime.run("hello")).output, "delivered-7");
  } finally {
    runtime.close();
  }
  assert.equal(serverCalls, 0);
});

test("exports reject missing hook bindings and preserve existing destinations", async (t) => {
  const temp = directory(t);
  const destination = path.join(temp, "bundle");
  const deployment = delivery.createAgentDeployment(
    config,
    environment({ hooks: { decideAction: () => ({ passthrough: true }) } }),
  );
  await assert.rejects(
    deployment.exportClient(destination),
    /require a hooksModule/,
  );
  assert.equal(fs.existsSync(destination), false);
  const plain = delivery.createAgentDeployment(config, environment());
  await assert.rejects(
    plain.exportClient(destination, {
      clientTools: { delivery_receipt: "../outside.mjs" },
    }),
    /relative/,
  );
  await assert.rejects(
    plain.exportClient(destination, {
      serverUrl: "https://user:secret@example.com",
    }),
    /credentials/,
  );
  fs.mkdirSync(destination);
  fs.writeFileSync(path.join(destination, "keep.txt"), "preserve");
  await assert.rejects(plain.exportClient(destination), /already exists/);
  assert.equal(
    fs.readFileSync(path.join(destination, "keep.txt"), "utf8"),
    "preserve",
  );
});

test("delivered synchronous hook modules preserve behavior across execution targets", async (t) => {
  const temp = directory(t);
  const bindings = path.join(temp, "bindings");
  fs.mkdirSync(bindings);
  fs.writeFileSync(
    path.join(bindings, "hooks.mjs"),
    '/** Override the terminal response. */\nexport default {decideAction:()=>({action:"final",content:"hook-result"})};',
  );
  const deployment = delivery.createAgentDeployment(
    config,
    environment({
      hooks: {
        decideAction: () => ({ action: "final", content: "hook-result" }),
      },
    }),
  );
  assert.equal((await deployment.run("hello")).output, "hook-result");
  const bundle = path.join(temp, "export");
  await deployment.exportClient(bundle, {
    bindingsDir: bindings,
    hooksModule: "hooks.mjs",
  });
  const { createAgent } = await import(
    pathToFileURL(path.join(bundle, "agent.js")).href
  );
  const runtime = await createAgent({ provider: environment().provider });
  try {
    assert.equal((await runtime.run("hello")).output, "hook-result");
  } finally {
    runtime.close();
  }
});

test("delivery HTTP host supports native execution, public bundle assets, authenticated proxying, and permission denial", async (t) => {
  const temp = directory(t);
  const seen = [];
  const env = environment();
  const deployment = delivery.createAgentDeployment(config, {
    ...env,
    provider: async (request) => {
      seen.push(request);
      return env.provider(request);
    },
  });
  const host = await deployment.serve({ authToken: "host-secret" });
  try {
    assert.equal(
      (await fetch(host.clientUrl)).headers.get("content-type"),
      "text/javascript",
    );
    const manifest = await (await fetch(host.manifestUrl)).json();
    assert.equal(manifest.agentId, config.id);
    assert.equal(
      (await fetch(`${host.url}/bundle/agent.wasm`)).headers.get(
        "content-type",
      ),
      "application/wasm",
    );
    const request = (route, body, authorized = true) =>
      fetch(host.url + route, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          ...(authorized ? { Authorization: "Bearer host-secret" } : {}),
        },
        body: JSON.stringify(body),
      });
    assert.equal(
      (await request("/run", { prompt: "hello" }, false)).status,
      401,
    );
    const result = await (await request("/run", { prompt: "hello" })).json();
    assert.equal(result.output, "delivered-7");
    assert.equal(
      (await request("/tools/execute", { "tool-name": "unregistered" })).status,
      403,
    );
    await request("/llm/call", {
      provider: "attacker",
      model: "attacker",
      messages_json: "[]",
    });
    assert.equal(seen.at(-1).model, config.model);
    assert.equal(seen.at(-1).provider, config.provider);
    const bundle = path.join(temp, "export");
    await deployment.exportClient(bundle, { serverUrl: host.url });
    assert.doesNotMatch(
      fs.readFileSync(path.join(bundle, "agent.json"), "utf8"),
      /host-secret/,
    );
    const { createAgent } = await import(
      pathToFileURL(path.join(bundle, "agent.js")).href
    );
    const runtime = await createAgent({ authToken: "host-secret" });
    try {
      assert.equal((await runtime.run("hello")).output, result.output);
    } finally {
      runtime.close();
    }
    assert.equal(
      (await fetch(`${host.url}/bundle/%2e%2e%2fpackage.json`)).status,
      404,
    );
  } finally {
    await host.close();
  }
});

test("cancellation and timeouts prevent a pending host response from committing to the component", async () => {
  const deployment = delivery.createAgentDeployment(
    { ...config, timeoutMs: 30 },
    { provider: () => new Promise(() => {}) },
  );
  const runtime = await deployment.createRuntime();
  try {
    const running = runtime.run("hello");
    await assert.rejects(runtime.run("overlap"), /active turn/);
    await assert.rejects(running, /timed out/);
    assert.equal(
      runtime
        .getState()
        .message_history.filter((message) => message.role === "assistant")
        .length,
      0,
    );
  } finally {
    runtime.close();
  }
});

test("Python-configured agent exports run with jco against the same Python provider and tools", async (t) => {
  const temp = directory(t);
  const bundle = path.join(temp, "python-export");
  const python =
    process.env.PYTHON ?? (process.platform === "win32" ? "python" : "python3");
  const child = spawn(
    python,
    [
      "-u",
      path.join(root, "tests/node/helpers/python-delivery-server.py"),
      bundle,
    ],
    {
      cwd: root,
      env: { ...process.env, PYTHONPATH: path.join(root, "clients/python") },
      stdio: ["pipe", "pipe", "pipe"],
    },
  );
  let stderr = "";
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  let host;
  try {
    host = await new Promise((resolve, reject) => {
      let output = "";
      const timer = setTimeout(() => {
        child.kill();
        reject(new Error(`Python delivery startup timed out: ${stderr}`));
      }, 20000);
      child.stdout.on("data", (chunk) => {
        output += chunk;
        /**
         * The helper prints its startup JSON only after writing the bundle, so the
         * first stdout lines may belong to unrelated diagnostics. Scan complete
         * lines until the host descriptor (`url` + `clientUrl`) arrives instead of
         * trusting the first line.
         */
        const lines = output.split("\n");
        output = lines.pop() ?? "";
        for (const line of lines) {
          let parsed;
          try {
            parsed = JSON.parse(line);
          } catch {
            continue;
          }
          if (typeof parsed?.url === "string" && typeof parsed?.clientUrl === "string") {
            clearTimeout(timer);
            resolve(parsed);
            return;
          }
        }
      });
      child.once("error", (error) => {
        clearTimeout(timer);
        reject(error);
      });
      child.once("exit", (code) => {
        clearTimeout(timer);
        reject(new Error(`Python delivery exited ${code}: ${stderr}`));
      });
    });
    const manifest = JSON.parse(
      fs.readFileSync(path.join(bundle, "manifest.json"), "utf8"),
    );
    verifyInventory(bundle, manifest);
    const result = await (
      await fetch(`${host.url}/run`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: '{"prompt":"hello"}',
      })
    ).json();
    assert.equal(result.output, "delivered-7");
    const { createAgent } = await import(
      pathToFileURL(path.join(bundle, "agent.js")).href
    );
    const runtime = await createAgent();
    try {
      const clientResult = await runtime.run("hello");
      assert.equal(clientResult.output, result.output);
      assert.equal(clientResult.stepsUsed, result.stepsUsed);
    } finally {
      runtime.close();
    }
    assert.equal((await fetch(host.clientUrl)).status, 200);
  } finally {
    const exited = new Promise((resolve) => child.once("exit", resolve));
    child.stdin.end();
    const timeout = setTimeout(() => child.kill(), 5000);
    await exited;
    clearTimeout(timeout);
  }
});
