# Antikythera Agent SDK

General-purpose agent runtime with multi-agent orchestration, MCP tool integration, and WebAssembly support.

## Installation

```bash
pip install antikythera-agent
```

For WASM execution support:

```bash
pip install antikythera-agent[wasm]
```

## Namespaces

- `antikythera_agent.core` — stable core (Agent, Orchestrator, SessionManager, PromptManager, types, get_version) — zero extra dependencies
- `antikythera_agent.plugin` — optional, wasm-heavy (WasmRuntime) — requires `wasmtime` via `pip install antikythera-agent[wasm]`

The root `antikythera_agent` re-exports both for backward compatibility (`from antikythera_agent import Agent` still works). New code should prefer the split imports:

```python
from antikythera_agent.core import Agent, Orchestrator, PromptManager
from antikythera_agent.plugin import WasmRuntime
```

## Quick Start

### Create an Agent

```python
from antikythera_agent.core import Agent
from antikythera_agent.server.provider import StubProvider

agent = Agent(
    provider="stub",
    model="demo",
    provider_resolver=lambda name: StubProvider('{"action":"final","content":"hi"}'),
)

result = agent.run("Hello")
print(result.output)
```

Without a `provider_resolver`, the built-in registry is used (default providers: `stub`, `ollama`). Runs execute the local tool loop over the bundled WASM, which requires the optional `[wasm]` extra.

### Multi-Agent Orchestration

```python
from antikythera_agent.core import AgentProfileConfig, Orchestrator, PromptManager, PromptConfig

prompts = PromptManager()
prompts.register(PromptConfig(
    id="coder",
    name="Coder",
    content="You are a software engineer.",
    tags=["engineering"]
))
prompts.register(PromptConfig(
    id="reviewer",
    name="Reviewer",
    content="You are a code reviewer.",
    tags=["quality"]
))

orchestrator = Orchestrator(execution_mode="concurrent", max_concurrent_tasks=4)

orchestrator.register_agent(AgentProfileConfig(
    id="coder",
    name="Coder",
    role="developer",
    system_prompt=prompts.get_content("coder")
))

orchestrator.register_agent(AgentProfileConfig(
    id="reviewer",
    name="Reviewer",
    role="reviewer",
    system_prompt=prompts.get_content("reviewer")
))

results = orchestrator.dispatch_many(["Write a sorting algorithm", "Explain big-O notation"])
for result in results:
    print(result.success, result.output)

budget = orchestrator.get_budget()
print(budget["consumed_steps"], budget["dispatched_tasks"])
```

Dispatch runs the local tool loop host-side on a fresh `WasmRuntime` per task; `get_budget()` reports factual counters from executed results.

### WASM Runtime (Server-Side)

```python
from antikythera_agent.plugin import WasmRuntime

runtime = WasmRuntime()
result = runtime.call_checked("init", '{"max_steps": 10}')
print(result)
```

The Python `WasmRuntime` executes the composite WASM locally in-process via wasmtime. It does not include the Runtime Bridge: core placement cannot be selected (client or server), there is no HTTP/SSE transport, and execution cannot be moved between a client and a server.

## Runtime Bridge Server (Python)

The Python SDK ships a drop-in Runtime Bridge server — a wire-protocol peer of `antikythera-server-runtime` (Rust). It serves the LLM proxy, SSE control channel, and the jco-transpiled bundle so a browser can load the composite from Python without bundling it locally.

### Scope: server-side peer

This package is a **server-side** wire peer only: LLM proxy, SSE control channel, static bundle serving, plus the optional `core@server` loop via wasmtime. It does not provide an SSE-client peer — that role belongs to the JavaScript client runtime (`antikythera-agent/runtime`). This scope decision is permanent until changed in the decision register (`documentation/DECISIONS_RUNTIME_BRIDGE.md`, D7).

```python
from antikythera_agent.server import createAgentServer, AgentServerOptions
```

`pip install antikythera-agent` stays zero-dependency (stdlib `ThreadingHTTPServer` transport, no wasmtime). `pip install antikythera-agent[wasm]` enables `core@server` mode (wasmtime) — `core@client` (static + wire) needs no wasmtime (D6).

### Programmatic — core@client (static + wire, no wasmtime)

```python
from antikythera_agent.server import createAgentServer

server = createAgentServer({
    "providers": {"stub": {"type": "stub", "response": '{"action":"final","content":"hi"}'}},
})
url = server.start()  # -> http://127.0.0.1:<ephemeral-port>
# ... serve to browser, then server.stop()
```

Pair with the JS client host (loads the bundle from the Python server via `componentBase`):

```javascript
import { createAgentRuntime } from 'antikythera-agent/runtime';

const runtime = await createAgentRuntime({
  serverUrl: url,
  componentBase: url + "/antikythera/v1/component/",
});
await runtime.connect();
const result = await runtime.runTurn("hello");
```

`componentBase` is the absolute URL of the bundle directory; the entry file is resolved from the server manifest `GET /antikythera/v1/component/manifest` (`entry: "antikythera-sdk.js"`), so the client never hardcodes the layout. Omit `componentBase` to keep the npm-bundled component (default, backward compatible).

Wire parity: Python server = drop-in peer — 7/7 wire-protocol cases and 3/3 E2E jco delivery cases pass against the same golden contract as the Rust server.

### CLI

```bash
python -m antikythera_agent.server --bind 127.0.0.1:0 --provider-stub '{"action":"final","content":"hi"}' --component-dir <path-to-jco-bundle>
# [server-runtime] HTTP wire bridge listening on http://127.0.0.1:<port>
```

`--bind` is `<addr>:<port>` (`:0` = ephemeral, actual port is printed in the `listening on` line); `--provider-stub` replaces the `stub` provider and makes it the default; `--component-dir` points at the jco bundle directory (omit to use the wheel's bundled `antikythera_agent/component`). Other flags mirror the Rust server: `--server-tool <name>:<json>` (registration = grant), `--allow-tool <name>`, `--client-id`, `--wasm-path`, `--max-steps`.

Wire contract: `documentation/WIRE_PROTOCOL.md`; decision register: `documentation/DECISIONS_RUNTIME_BRIDGE.md` (D1–D7).

### Load Prompts from JSON

```python
from antikythera_agent.core import PromptManager

# Load from file
prompts = PromptManager.from_file("prompts.json")

# Or load from string
prompts = PromptManager.from_json('[{"id":"agent","name":"Agent","content":"You are helpful."}]')
```

## Requirements

- Python 3
- wasmtime (optional, for WASM execution; the supported minimum is pinned in pyproject.toml)

## License

MIT
