# antikythera-agent

General-purpose agent runtime with multi-agent orchestration, MCP tool integration, and WebAssembly support.

## Installation

```bash
npm install antikythera-agent
```

## Namespaces

- `antikythera-agent/core` — stable core (`PromptManager`, `SessionManager`, `getVersion`, types) — no wasm
- `antikythera-agent/plugin` — optional, wasm-heavy (`createAgentRuntime`, `Orchestrator`, `component`) — needs the WASM bundle

The root `antikythera-agent` re-exports both for backward compatibility (`import { PromptManager } from 'antikythera-agent'` still works). New code should prefer the split imports:

```javascript
import { PromptManager } from 'antikythera-agent/core';
import { createAgentRuntime } from 'antikythera-agent/plugin';
```

## Quick Start

### Browser Usage

```javascript
import { PromptManager } from 'antikythera-agent/core';

const prompts = new PromptManager();

prompts.register({
  id: 'assistant',
  name: 'Assistant',
  content: 'You are a helpful assistant.',
  tags: ['general']
});

const content = prompts.getContent('assistant');
```

### WASM via jco component

The browser WASM path is a **composite** WASI component (`wasm32-wasip2`) transpiled with `@bytecodealliance/jco`: the `antikythera-sdk` runner component is composed with the embedded `antikythera-toolrunner` component and the `antikythera-default-hooks` passthrough provider (`wasm-tools compose`), so builtin tools execute inside the module without host round-trip and the pipeline defaults match the SDK alone. Import the `runner` namespace from `antikythera-agent/component`:

```javascript
import { runner } from 'antikythera-agent/component';

// Initialize the agent runner � returns the raw session id (plain string, not JSON)
const sessionId = runner.init(JSON.stringify({ session_id: 's1' }));

// Get the agent state for a session
const state = runner.getState(sessionId);
```

The `runner` namespace exposes 16 camelCase functions (all payloads are JSON strings): `init`, `prepareUserTurn`, `commitLlmResponse`, `commitLlmStream`, `processLlmResponseForSession`, `processToolResultForSession`, `appendLlmChunk`, `drainEvents`, `getState`, `resetSession`, `sweepIdleSessions`, `registerTools`, `getToolsPrompt`, `setContextPolicy`, `getTelemetrySnapshot`, `getSloSnapshot`.

> The composite also imports one host-supplied interface, `antikythera:agent-sdk/runtime-hooks@1.0.0` (runtime pipeline-hook decisions), which `jco` maps to `component/runtime-hooks.js` (default passthrough stub; inject a provider via `globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__`). For automatic client�server connectivity (LLM proxy, tool routing, hook decisions over HTTP+SSE), use the high-level host runtime: `createAgentRuntime` from `antikythera-agent/runtime` � see `documentation/WIRE_PROTOCOL.md`.

> Note: the transpiled module uses top-level await � set your bundler build target to ES2022 (e.g. Vite `build.target: 'es2022'`).

### Loading the component from a server (`componentBase`)

When the jco bundle is served by an Antikythera server (e.g. the Python
bridge server) instead of being bundled with this package, point
`createAgentRuntime` at the bundle directory with `componentBase`:

```javascript
import { createAgentRuntime } from 'antikythera-agent/runtime';

const runtime = await createAgentRuntime({
  serverUrl: 'http://localhost:8000',
  componentBase: 'http://localhost:8000/antikythera/v1/component/',
});
```

`componentBase` is the absolute URL of the bundle directory; the entry file
name is resolved from the server manifest (`GET /antikythera/v1/component/manifest`,
see `documentation/WIRE_PROTOCOL.md` �2.6), so the client never hardcodes the
server's bundle layout. When `componentBase` is omitted the runtime keeps
loading the bundled component (default, backward compatible � the no-option
path makes no network request). Consumers that already hold a runner namespace
can inject it directly with the `runner` option to skip the import entirely;
injection takes precedence over `componentBase`.

### Multi-Agent Orchestration

```javascript
const { Orchestrator } = require('antikythera-agent/plugin');

const orchestrator = new Orchestrator({
  serverUrl: 'http://localhost:8000', // required
  executionMode: 'concurrent',
  maxConcurrentTasks: 2
});

orchestrator.registerAgent({
  id: 'coder',
  name: 'Coder',
  role: 'developer',
  systemPrompt: 'You are a software engineer.'
});

orchestrator.registerAgent({
  id: 'reviewer',
  name: 'Reviewer',
  role: 'reviewer',
  systemPrompt: 'You are a code reviewer.'
});

// Dispatching acquires and connects one client-core runtime per profile
// automatically � there is no separate connect() step.
const results = await orchestrator.dispatchMany([
  'Write a sorting algorithm',
  'Explain big-O notation'
]);

for (const result of results) {
  console.log(result.taskId, result.success, result.output);
}

console.log(orchestrator.getBudget()); // { consumedSteps, dispatchedTasks, ... }

// Reset every recorded session (idempotent), then close the live runtimes.
await orchestrator.cancel();
orchestrator.close();
```

`pipeline(tasks)` chains tasks sequentially, feeding each output into the next prompt and stopping at the first failed task. The runtime seam is injectable via the `runtimeFactory` option for consumers that need custom client-core runtimes.

### Load Prompts from JSON

```javascript
const { PromptManager } = require('antikythera-agent/core');

// Load from file
const prompts = PromptManager.fromFile('prompts.json');

// Or load from string
const prompts = PromptManager.fromJSON('[{"id":"agent","name":"Agent","content":"You are helpful."}]');
```

## Package Contents

| File | Description |
|:-----|:------------|
| `index.js` | Main re-export (backward compatible, re-exports `core` + `plugin`) |
| `core/` | Core surface (`PromptManager`, `SessionManager`, `getVersion`) — no wasm |
| `plugin/` | Plugin surface (`Orchestrator`, `createAgentRuntime`) — wasm-heavy |
| `orchestrator.js` | Multi-agent `Orchestrator` (requires `serverUrl`; `dispatch` / `dispatchMany` / `pipeline` / `getBudget` / `cancel` / `close`) |
| `component/` | Composite WASI component (SDK runner + embedded toolrunner + default-hooks) transpiled with jco � ESM bindings, namespace `runner` (camelCase), semantic core-module names (`runner`, `tool-registry`, `logic-hooks-passthrough`, `support-N`) assigned by `src/scripts/post-transpile-rename.mjs`, WASI stubs under `wasi-stubs/` |
| `runtime/` | Host runtime bridge (`createAgentRuntime`) |

## Requirements

- Node.js with ESM support

## License

MIT
