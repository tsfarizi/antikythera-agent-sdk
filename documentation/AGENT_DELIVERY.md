# Configured agents on the server and client

Prepare one deployment from an agent configuration and its host capabilities.
Run it on the server, export it to a static directory, or start a delivery host
that serves the client package and the capabilities that remain on the server.
The Python host uses the native Wasmtime Component Model API. The TypeScript
server and delivered browser client use private instances of the same component
through jco. Exporting requires no local Rust compiler or jco installation.

The SDK supplies the prebuilt composed component and its transpiled factory.
Python callbacks are retained by the host; they are not compiled into the
component. Client tools and hooks use explicit JavaScript bindings. This follows
jco's [instantiation API](https://bytecodealliance.github.io/jco/transpiling.html),
which supplies host imports independently for each component instance.

## Python: reuse an existing Agent

```python
from pathlib import Path
from antikythera_agent import Agent
from antikythera_agent.server.provider import OllamaProvider
from antikythera_agent.tools import tool

provider = OllamaProvider(model="my-model")

@tool(name="account_lookup", description="Look up an account summary")
def account_lookup(args):
    return {"account_id": args["account_id"], "status": "active"}

agent = Agent(
    provider="ollama",
    model="my-model",
    system_prompt="Answer account questions using the registered tool.",
    provider_resolver=lambda name: provider,
)
agent.register_tool(account_lookup, allow=True)

deployment = agent.deployment(agent_id="account-assistant")
result = deployment.run("Check account 42")

# Create an independently usable client directory for an existing host.
deployment.export_client(
    Path("dist/account-assistant"),
    server_url="http://127.0.0.1:8000",
)

# Or serve the package and its host capabilities together.
host = deployment.serve(port=8000)
print(host.client_url)
print(host.manifest_url)
# Keep the application alive while serving; call host.close() on shutdown.
```

`AgentDeployment.from_agent(agent, agent_id=...)` is the equivalent explicit
constructor. It captures the configuration, tool definitions, allow flags,
provider resolver, and instance-owned hooks without copying conversation state.
`deployment.run()` uses a deployment-owned session and serializes consecutive
calls. `create_agent()` creates an independent native agent when the application
needs separate sessions. Provider instances and handler closures can still refer
to shared application resources; their thread safety belongs to the application.

## TypeScript: bind the configured server environment

```typescript
import { createAgentDeployment } from 'antikythera-agent';

const deployment = createAgentDeployment({
  id: 'account-assistant',
  provider: 'ollama',
  model: 'my-model',
  systemPrompt: 'Answer account questions using the registered tool.',
  maxSteps: 8,
}, {
  provider: async (request, { signal }) => {
    const response = await fetch('http://127.0.0.1:11434/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        model: request.model,
        messages: JSON.parse(request.messages_json),
        stream: false,
      }),
      signal,
    });
    if (!response.ok) throw new Error(`Provider HTTP ${response.status}`);
    const body = await response.json();
    return { content: body.message.content };
  },
  tools: [{
    definition: { name: 'account_lookup', description: 'Look up an account summary' },
    handler: async (input) => ({ account: input, status: 'active' }),
    allow: true,
  }],
});

const result = await deployment.run('Check account 42');
const host = await deployment.serve({ port: 8000 });
console.log(host.clientUrl);

await deployment.exportClient('dist/account-assistant', {
  serverUrl: host.url,
});

// On application shutdown:
await host.close();
await deployment.close();
```

The API is also exported from `antikythera-agent/delivery`. Configuration has
`id`, `provider`, `model`, `systemPrompt`, `maxSteps`, and `timeoutMs`; executable
provider, tool, and hook bindings belong to the separate environment argument.
Provider callbacks return `{ content: string }`. Tool handlers return a JSON
value and may be async in JavaScript. Python handlers are synchronous. Runtime
hooks are synchronous in both hosts because the component imports are synchronous.

Existing TypeScript `AgentConfig` values can be spread into the deployment config
with an added `id`. The existing `timeout` field is accepted as an alias for
`timeoutMs`; the explicit `timeoutMs` value takes precedence. Existing `defineTool`
handlers can be reused as `{ tool: existingTool, allow: true }` entries, preserving
the separation between registration and permission.

`createRuntime()` creates a private jco instance. Its `run()` rejects overlapping
turns, applies the configured timeout, and accepts an `AbortSignal`.
`deployment.run()` serializes turns and returns execution errors through
`success: false`, like Python's Agent. Invalid prompts remain caller errors.
HTTP `/run` creates a fresh private session for each request; it does not expose
the deployment's local conversation to another caller. Session history is not
included in exported bundles.

## Run the delivered agent in a browser or Node.js

```javascript
import { createAgent } from './account-assistant/agent.js';

const agent = await createAgent();
try {
  const result = await agent.run('Check account 42');
  console.log(result.output);
} finally {
  agent.close();
}
```

Serve the export directory over HTTP for browser module loading. Node.js can
import `agent.js` from a local directory; its adjacent `package.json` declares
ES modules. The generated `agent.d.ts` supplies standalone TypeScript types.
Each call to `createAgent()` owns its hooks, registry, memory, and session.
The default client executes the agent loop locally and calls the configured
host for LLM inference and permitted server-owned tools. The protocol does not
require the legacy SSE bridge to run a turn.

Credentials are supplied when creating the client instance:

```javascript
const agent = await createAgent({ authToken: tokenFromYourApplication });
```

The export never inserts the host's auth token or provider credentials into
the public configuration. Agent prompts, tool definitions, and files in the
explicit binding directory are public delivery assets.

## Move selected capabilities into the client

Provide a binding directory and map registered tool names to relative JS module
paths. Each module exports a default handler or a named `handler`. The directory
is copied recursively, including relative module dependencies and assets;
symbolic links are rejected. External dependencies must be browser-resolvable
or bundled by the application before export.

```javascript
/** bindings/account-lookup.mjs: client implementation of the configured capability. */
export default function accountLookup(input) {
  return { account_id: input.account_id, status: 'active' };
}
```

```python
deployment.export_client(
    Path("dist/client-account-assistant"),
    server_url=host.url,
    bindings_dir=Path("bindings"),
    client_tools={"account_lookup": "account-lookup.mjs"},
)
```

```typescript
await deployment.exportClient('dist/client-account-assistant', {
  serverUrl: host.url,
  bindingsDir: 'bindings',
  clientTools: { account_lookup: 'account-lookup.mjs' },
});
```

The mapping changes where that tool executes while preserving its definition
and allow flag. Tools without a mapping remain on the server. Registering a
tool does not allow it: both the client loop and host check the captured policy
before dispatch. Built-in tools implemented by the composed component retain
their existing component behavior; application tools should use distinct names.

For hooks, provide `hooks_module="hooks.mjs"` in Python or
`hooksModule: 'hooks.mjs'` in TypeScript. The module exports a default hooks object
or a named `hooks` object with camelCase methods. If server hooks exist and no
client binding is supplied, export fails instead of dropping the hook behavior.

Clients may supply `provider`, `tools`, and `hooks` callbacks directly to
`createAgent()`. With local provider callbacks and local bindings for every
required tool, execution needs no delivery host. This moves the agent runtime
and host capabilities; model weights are not included by the SDK. Browser
filesystem, network, and hardware capabilities remain subject to the browser
and the provided adapters.

## Export artifacts and deployment host routes

| Artifact | Purpose |
| --- | --- |
| `agent.json` | Versioned configuration, tool permissions, execution locations, and client binding paths. |
| `agent.js`, `agent.d.ts` | Configured client entry and standalone TypeScript declarations. |
| `agent.wasm` | Original composed WASM Component Model artifact. |
| `component/` | jco factory, core modules with descriptive role names, and WASI host adapters. |
| `runtime.mjs` | Shared JS tool loop used by Node server execution and both languages' client exports. |
| `bindings/` | Explicitly selected client capability modules and dependencies. |
| `manifest.json` | Artifact paths, sizes, SDK/schema versions, and SHA-256 inventory; it does not hash itself. |

Export destinations must be new directories. Files are assembled in a staging
directory and installed by rename, with bounded retries for temporary Windows
sharing failures. A failed export does not overwrite an existing user directory.
The inventory supports independent verification; it is not a signature or an
automatic trust check performed by the browser loader.

| Route | Behavior |
| --- | --- |
| `POST /run` | Execute the configured agent in a fresh server session. Body: `{ "prompt": "..." }`. |
| `POST /llm/call` | Invoke the deployment's provider; provider and model selection are pinned by the host. |
| `POST /tools/execute` | Execute only registered, permitted server tool bindings using the SDK wire tool-call envelope. |
| `GET /bundle/*` | Serve the generated public client environment with JS/WASM/asset MIME types. |

`serve()` / `serve(...)` returns host URLs and a `close()` lifecycle operation.
Python also supports a context manager. These hosts expose JSON request/response
execution; applications needing SSE continue to use the existing runtime bridge.
Native Python execution failures and JS `deployment.run()` failures return
agent results; the low-level JS client runtime rejects failures. `/run` mirrors
the result form. HTTP validation/auth/permission failures use 400/401/403.

Non-loopback binds require an auth token. Use `allowed_origins` / `allowedOrigins`
for cross-origin browser applications and `public_url` / `publicUrl` when mounted
behind a reverse proxy. A proxy with a path prefix must remove that prefix before
forwarding requests. Bundle assets are public; runtime POST routes enforce the
configured token. JSON bodies are limited to 1 MiB. JS callbacks receive a
cancellation signal; custom host code is responsible for stopping its own work.
Native Python provider transports use the configured timeout, and custom Python
callbacks must implement their own time bounds.

## Validation

- `tests/node/delivery.test.mjs`: real jco sessions, server/client tool loops,
  portable hook bindings, export hashes, auth, cancellation, and a Python export
  executed through JavaScript against its original Python host.
- `tests/python/test_delivery.py`: native component execution, captured tools,
  permissions, hooks, exports, HTTP routes, and independent request sessions.
- `tests/typescript/delivery.ts`: compiler verification of the public server and
  standalone client declarations.
- `tests/node/helpers/delivery-browser-host.mjs`: serves the browser fixture;
  open the printed URL and require `PASS`, two steps, and `execution: browser`.
- `tests/packaging/pack-smoke.mjs`: requires delivery code, declarations, the raw
  component, and identical shared JS runtime/type assets in npm and Python.
