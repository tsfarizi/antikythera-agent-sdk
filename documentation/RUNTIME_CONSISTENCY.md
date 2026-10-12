# Runtime consistency and Component Model ownership

The Rust and Python hosts execute the same WASI Component Model composite that
generates the JavaScript client. Client execution uses a private jco instance,
with instance-owned WIT imports, sessions, tool registry, and hooks.

## Build and artifact flow

1. Build the SDK, toolrunner, and default-hooks crates for `wasm32-wasip2`, with
   `--no-default-features --features component`.
2. Compose them into `dist/antikythera-sdk.wasm` using `wasm-tools compose`.
3. Run `node src/scripts/transpile-client.mjs`. This generates the legacy ESM
   bundle and an async jco factory using `--instantiation async --no-wasi-shim`.
   Both bundles receive descriptive core-module names and updated loader
   references through `post-transpile-rename.mjs`; see
   [the module naming contract](CORE_MODULE_NAMES.md).
4. The script copies the composite to the Python package as `antikythera.wasm`
   and copies the complete client bundle into its `component/` directory.

From the repository root:

```sh
task build
task transpile
```

`task transpile-clean` removes generated entries while preserving the source
adapter and WASI stubs. Repeating transpilation does not rewrite an identical
Python component that may already be mapped by a running Wasmtime instance.
Changed components are installed by rename rather than partial overwrite.

The source of the client adapter is
`clients/npm/antikythera-sdk/component/instantiate.js`. Generated bindings live
under `component/factory/`. The adapter shares compiled immutable
`WebAssembly.Module` objects, while each instantiation owns mutable WASM state.
It supplies explicit runtime-hook imports and the existing WASI stubs.

Rust uses Wasmtime's Component Model linker. Python uses
`wasmtime.component.Component` and `Linker.add_wasip2`, with serialized access
to each Store. Both consume WIT interfaces rather than a core-module ABI.
The npm and PyPI release paths now use the same verified composite artifact;
Python's embedded WASM is not selected from a separately rebuilt server job.

## JavaScript client

For configured agents that should run on a server or be delivered as a client
environment, use `AgentDeployment`. Python captures an existing Agent through
`agent.deployment(agent_id=...)`; TypeScript exposes `createAgentDeployment`.
Both export the same versioned configuration, composed WASM, jco factory, and
explicit client bindings. See [configured-agent delivery](AGENT_DELIVERY.md).

```javascript
import { createAgentRuntime } from 'antikythera-agent/runtime';

const runtime = await createAgentRuntime({
  core: 'client',
  serverUrl: 'http://127.0.0.1:8000',
  timeoutMs: 60000,
  // authToken: 'configured-bridge-token',
  hooks: {
    decideAction: () => ({ passthrough: true }),
  },
});
await runtime.connect();
try {
  const result = await runtime.runTurn('Hello');
  console.log(result.content);
} finally {
  runtime.close();
}
```

`createAgentRuntime` automatically uses the factory. Low-level consumers can
import `createRunner` from `antikythera-agent/component/instantiate`. Hooks
must be synchronous and return a JSON decision or JSON string; asynchronous
hook results are rejected explicitly. An injected `runner` remains supported,
and its instance ownership is the caller's responsibility.

The `antikythera-agent/component` singleton and global hook helpers remain
available for existing consumers. They retain legacy ownership semantics;
use the factory when independent sessions and hooks are required.

`componentBase` points to a complete server-served bundle containing
`instantiate.js`, `factory/`, and `wasi-stubs/`. The peer manifest is validated
before loading the factory. Browsers can import HTTP ESM directly. Node's
default ESM loader requires a local bundle or a bundler for remote modules.

## Execution guarantees

| Area | Resulting behavior |
| --- | --- |
| Concurrent budgets | JavaScript and Python reserve task and step capacity before execution, so concurrent tasks cannot all pass admission against the same remaining budget. Attempted steps are counted when a task fails or is cancelled. |
| Agent selection | Dispatch accepts `agentId` in JavaScript and `agent_id` in Python. Duplicate profile IDs and invalid limits fail explicitly. |
| Hook isolation | Factory imports belong to the client instance. Python resolves hooks from the owning runtime rather than swapping a process-wide provider during a loop. |
| Provider streams | Rust buffers complete NDJSON/SSE records before decoding UTF-8. Python supports Ollama and OpenAI-compatible HTTP streaming. Provider errors and missing completion markers fail explicitly. |
| Commit content | The complete HTTP LLM response is committed to the runner. SSE tokens are progress notifications and cannot replace the response with a partial stream. |
| Cancellation | JavaScript aborts pending HTTP requests and checks cancellation before committing or processing tools. Python registers cancellation before execution and checks it at loop boundaries. |
| SSE readiness | `connect()` waits for the initial SSE connection. Repeated concurrent connection attempts share one pending connection. A stale Python connection cannot unregister its replacement. |
| Storage | IDs are validated before path construction. Writes use a same-directory temporary file, flush, and rename rather than exposing a partially written session file. |
| Permission quota | Per-session LLM quotas retain existing counters when capacity is full; new tracked sessions are denied instead of clearing the quota map. |
| Distribution | Required export targets, factory bindings, and WASM modules are checked in the npm package and Python wheel. Component acceptance tests require build artifacts and fail if they are absent. |

## Bridge access and Python integrations

Non-loopback bridge binds require an authentication token. Configure
`ANTIKYTHERA_AUTH_TOKEN` and comma-separated `ANTIKYTHERA_ALLOWED_ORIGINS` for
the Rust or Python CLI, or pass `auth_token` and `allowed_origins` to
`createAgentServer` in Python. JavaScript sends the token via `authToken`.
Origins are explicitly allowed; a token alone does not grant an origin.
Static component files are public so browser module imports can load them.
The bridge token is a service credential, not a per-user session authorization
system. Remote deployments must provide their own TLS and user authorization.

Python now provides `OpenAiProvider` and an MCP tool client using stdio or
Streamable HTTP. Registration and execution permission remain separate:

```python
from antikythera_agent.server import createAgentServer

server = createAgentServer({"auth_token": "configured-bridge-token"})
server.connect_mcp_server({"url": "http://127.0.0.1:9000/mcp", "timeout": 30})
server.gate.allow_tool("mcp", "echo")
server.gate.set_llm_quota(20)
try:
    server.start()
    # Keep the server running for the application lifecycle.
finally:
    server.stop()
```

For stdio, supply `command`, `args`, and optional `env` instead of `url`.
MCP supports initialization, paginated tool discovery, tool calls, JSON/SSE
HTTP responses, negotiated session headers, and request timeouts. This is a
tool client; server-initiated sampling and elicitation are outside its scope.

## Validation and limits

The local validation uses real component artifacts, Wasmtime stores, jco
bindings, loopback HTTP/SSE servers, and stdio subprocesses. Provider fixtures
exercise protocol behavior without requiring a paid API or an Ollama model.

- Full Node suite: 97 tests, including Rust/Python bridge integration and
  server-served component loading. The focused npm suite contains 59 tests.
- Full Python suite: 361 tests, including independent hooks, cancellation,
  provider streaming, authentication, quotas, MCP, and SSE replacement.
- Rust: server-runtime library, bridge and smoke suites; storage regressions;
  the full `antikythera-tests` suite, including ten WIT compatibility checks.
- Static checks: scoped Rust Clippy with warnings denied; Python Ruff and Mypy.
- Packaging: npm export/file checks and an actual Python wheel build.
- Browser: `tests/node/browser-component-smoke.html` loads the generated factory in
  a real browser and verifies independent hooks and session reset behavior.

SSE reconnects with a fresh subscription. There is no historical event replay;
refresh tools after a disconnect if registry state may have changed. An
interrupted remote tool or hook request can still fail at its deadline. Full
HTTP responses remain authoritative for completed LLM calls.

Cancellation is cooperative for local tool handlers and Python's blocking
provider calls. It prevents later commits once observed, but does not forcibly
interrupt an arbitrary handler or Python thread. Provider deadlines bound
network calls. Concurrent turns on the same client runtime are rejected.

GitHub Actions and publication were not executed locally. The workflows have
been updated to build required components before acceptance checks and to
validate runtime artifacts before staging them for publication.
