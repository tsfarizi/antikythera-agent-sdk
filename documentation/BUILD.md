# BUILD

This guide covers the commands that match the current workspace layout and tooling.

## Build map

```mermaid
flowchart TD
    SRC[Workspace source] --> CARGO[cargo build --workspace]
    SRC --> WIT[cargo run -p build-scripts --release -- validate]
    WIT --> SDK[cargo component build -p antikythera-sdk ... --features component]
    WIT --> TR[cargo component build -p antikythera-toolrunner ... --features component]
    WIT --> DH[cargo component build -p antikythera-default-hooks ... --features component]
    SDK --> COMPOSE[wasm-tools compose SDK + toolrunner + default-hooks]
    TR --> COMPOSE
    DH --> COMPOSE
    COMPOSE --> DIST[dist/antikythera-sdk.wasm - composite]
    DIST --> JCO[npx jco transpile dist/antikythera-sdk.wasm --out-dir clients/npm/antikythera-sdk/component]
    JCO --> NPM[clients/npm/antikythera-sdk/component/ ESM bindings]
    DIST --> HARNESS[examples/component-harness - wasmtime server proof]
    DIST --> SERVER[cargo build -p antikythera-server-runtime - HTTP/SSE wire bridge]
    SERVER --> RUNTIME[clients/npm/antikythera-sdk/runtime - antikythera-agent/runtime JS host]
```

## What you can build

| Target | Status | Notes |
|:-------|:------:|:------|
| Workspace crates | ? | `cargo build --workspace` — all framework crates + tests + scripts |
| `src/antikythera-sdk` component | ? | Standalone intermediate via `cargo-component` + `wasm32-wasip2` (feature `component`); imports `tool-registry` + `logic-hooks` — **not** a consumable deliverable |
| `antikythera-toolrunner` component | ? | Standalone intermediate via `cargo-component` + `wasm32-wasip2` (feature `component`); exports `tool-registry` (builtin tools) |
| `antikythera-default-hooks` component | ? | Standalone intermediate via `cargo-component` + `wasm32-wasip2` (feature `component`); exports `logic-hooks` (no-op passthrough) |
| **Composite WASM deliverable** | ? | `wasm-tools compose` SDK + toolrunner + default-hooks ? `dist/antikythera-sdk.wasm` (exports `runner`; imports exactly one non-WASI interface — `runtime-hooks` — plus WASI) |
| Browser JS bindings | ? | `jco transpile` of the **composite** ? `clients/npm/antikythera-sdk/component/` (ESM, namespace `runner`) |
| **Server host runtime** | ? | `cargo build -p antikythera-server-runtime` — wasmtime host + HTTP/SSE wire bridge (lib + bin) |

## Prerequisites

- recent stable Rust toolchain (edition 2024)
- `cargo-component` for component builds
- `wasm-tools` for composing the SDK, toolrunner, and hooks components
- Node.js with ESM support and `@bytecodealliance/jco` (installed at repo root; the version pinned in the root package.json) for the browser JS bindings
- Optional: `task` for the helpers in `Taskfile.yml`

## Native builds

### Build all workspace crates

```bash
cargo build --workspace
```

### Build release artifacts

```bash
cargo build --workspace --release
```

## WASM component build

### Validate WIT

```bash
cargo run -p build-scripts --release -- validate
```

This validates the checked-in WIT file against Rust source types.

### Build the composite WASM component

The WASM deliverable is **composite** (three members): the SDK component imports `tool-registry` and `logic-hooks`; the toolrunner component exports `tool-registry`; the default-hooks component exports `logic-hooks` (no-op passthrough). `wasm-tools compose` wires all three. Build the three components, then compose:

```bash
cargo component build -p antikythera-sdk --release --target wasm32-wasip2 \
  --no-default-features --features component
cargo component build -p antikythera-toolrunner --release --target wasm32-wasip2 \
  --no-default-features --features component
cargo component build -p antikythera-default-hooks --release --target wasm32-wasip2 \
  --no-default-features --features component
```

The helper binary in `src/scripts/build-component.rs` validates WIT conformance against Rust source types.

Expected component output is produced under:

```text
target/wasm32-wasip2/release/
```

Compose into the canonical composite artifact (the component artifacts are copied to kebab-case names first because `wasm-tools compose` rejects underscores):

```bash
mkdir -p dist
cp target/wasm32-wasip2/release/antikythera_toolrunner.wasm \
  target/wasm32-wasip2/release/antikythera-toolrunner.wasm
cp target/wasm32-wasip2/release/antikythera_default_hooks.wasm \
  target/wasm32-wasip2/release/antikythera-default-hooks.wasm
wasm-tools compose target/wasm32-wasip2/release/antikythera_sdk.wasm \
  -d target/wasm32-wasip2/release/antikythera-toolrunner.wasm \
  -d target/wasm32-wasip2/release/antikythera-default-hooks.wasm \
  -o dist/antikythera-sdk.wasm
```

Canonical artifact name for CI/release packaging:

```text
dist/antikythera-sdk.wasm
```

`task compose` wraps the copy + compose steps (and depends on the component builds); `task build` runs the full composite build (WIT validation + all three components + compose).

> **Never transpile or embed the standalone SDK component.** `target/wasm32-wasip2/release/antikythera_sdk.wasm` still imports `tool-registry` and `logic-hooks`; jco transpilation of it yields a module with unmet imports that fails Node smoke tests. Always consume the composite.

### Verify the composite server-side

`src/examples/component-harness` is a wasmtime server binary that loads a component path (default `dist/antikythera-sdk.wasm`) and proves the builtin tool `echo` executes inside the composite with no host round-trip. The harness wires the `runtime-hooks` import into its linker (deterministic provider; default passthrough) and exposes `--runtime-hooks=passthrough|override|deny` plus dedicated probes:

```bash
cargo run -p component-harness --release -- --probe=runtime-hooks-passthrough
cargo run -p component-harness --release -- --probe=runtime-hooks-override
cargo run -p component-harness --release -- --probe=runtime-hooks-deny
cargo run -p component-harness --release -- --probe=runtime-hooks-disabled
```

`runtime-hooks-passthrough` proves the default stub keeps the SDK decision; `runtime-hooks-override` proves a runtime `decide-action` override forces `action=final`; `runtime-hooks-deny` proves a denial aborts fail-closed; `runtime-hooks-disabled` proves `runtime_hooks_enabled: false` skips the runtime provider entirely.

```bash
cargo run -p component-harness --release
```

Pass a custom hooks component and `--expect=final` to prove a host-authored `decide-action` override forces `action=final` and no tool executes:

```bash
cargo run -p component-harness --release -- <custom-hooks>.wasm --expect=final
```

### Host-authored logic hooks

The composite calls three plug-and-play pipeline hooks (`prepare-turn`, `decide-action`, `handle-tool-result`); the default composite embeds `src/plugin/antikythera-default-hooks`, a no-op passthrough provider. To customize pipeline decisions without changing the SDK, author a hooks component from the template `examples/logic-hooks-template/`:

- Edit only the three `custom_*` functions in `src/lib.rs` (`custom_prepare_turn`, `custom_decide_action`, `custom_handle_tool_result`). `None` = passthrough (SDK default); `Some(json)` = override. To abort the operation (Err-path), return an error from the matching `Guest` method in `component.rs`.
- Build the component:

```bash
cargo component build -p logic-hooks-template --release --target wasm32-wasip2 \
  --no-default-features --features component
```

- Compose it into the SDK in place of the default-hooks member (`wasm-tools compose ... -d <hooks>.wasm`; `task compose-hooks-custom` wraps this flow). The artifact exports `antikythera:agent-sdk/logic-hooks`.
- Verify with the harness (`--expect=final`):

```bash
cargo run -p component-harness --release -- <hooks>.wasm --expect=final
```

`examples/logic-hooks-example/` is a filled-in probe: its `decide-action` always returns `{"action":"final","content":"hook-forced-final"}`, so the harness `--expect=final` mode asserts the committed action is `final` and no tool executes. See [`WASM_ARCHITECTURE.md`](WASM_ARCHITECTURE.md) — Logic hooks for the full contract (hook points, passthrough/override/fail-closed semantics, state ownership).

### Authoring a drop-in logic core

The deepest customization is replacing the runner itself. A host-authored logic core that exports the same `runner` contract (world `logic-core-component` — the same 16 functions, the same JSON-string semantics) loads wherever the SDK composite loads, with zero host code changes. See [`WASM_ARCHITECTURE.md`](WASM_ARCHITECTURE.md) — Drop-in logic core (swap-able runner) for the contract, the optional `host-imports` / `tool-registry` imports, and the pruning rule. A logic core that uses the `host-imports` escape hatch additionally requires the host wiring below — Host-imports wiring.

Start from the template `examples/logic-core-template/`:

- Edit only the 14 `custom_*` functions in `src/lib.rs` (one per runner function except `get-state` / `reset-session`, which are fixed in-memory store plumbing). `None` = template default: `init` / `get-state` / `reset-session` work out of the box; every other runner function returns the structured error `{"error":"not implemented","function":"<kebab-case-name>"}`.
- Build the component. The logic core is consumed directly as a standalone artifact — it is never composed into `dist/`:

```bash
task build-logic-core-template
# or, for the filled-in probe:
task build-logic-core-example
```

(The tasks wrap `cargo component build -p logic-core-template|logic-core-example --release --target wasm32-wasip2 --no-default-features --features component`; artifacts land under `target/wasm32-wasip2/release/` — `logic_core_template.wasm` / `logic_core_example.wasm`. Unlike the composite members, no kebab-case copy is needed: the logic core is never composed.)

- Verify server-side with the harness final probe:

```bash
cargo run -p component-harness --release -- <logic-core>.wasm --expect=final --expect-content=echo-agent-done
```

`examples/logic-core-example/` is a deterministic "echo-agent": its commit always returns `{"action":"final","content":"echo-agent-done",...}`, so the command above asserts the committed action is `final` with that content and no tool executes. Use `--expect=notimpl` to probe template holes and assert the structured not-implemented error.

- Verify client-side: because a self-contained logic core imports only WASI, it transpiles with jco like the composite does (the same 12 WASI stub mappings; the `runtime-hooks` mapping is only needed when the component actually imports it, see the transpile section below) and runs in the Node probe and the Vite bundle.

### Host-imports wiring

A logic core that uses the `host-imports` escape hatch (world `logic-core-component`; import name `antikythera:agent-sdk/host-imports@1.0.0`) needs a host that implements the five import functions — `call-llm`, `save-state`, `load-state`, `emit-tool-call`, `log-message` — behind permission gates. The reference implementation is `src/examples/component-harness`.

Build the host-imports probe artifact (no task wraps this crate; use the direct command):

```bash
cargo component build -p logic-core-host-example --release --target wasm32-wasip2 \
  --no-default-features --features component
```

Artifact: `target/wasm32-wasip2/release/logic_core_host_example.wasm` (exports `runner`; imports `antikythera:agent-sdk/host-imports@1.0.0`).

**Server.** `src/examples/component-harness` registers the import into the wasmtime linker with `wasmtime::component::bindgen!` plus the generated `add_to_linker` (in the harness: `Harness::add_to_linker::<_, wasmtime::component::HasSelf<HostState>>(&mut linker, |state| state)`), alongside `wasmtime_wasi::p2::add_to_linker_sync` for WASI; a manual `Linker::func_wrap` per function is the equivalent when wiring by hand. The `Host` impl applies the permission gates (call-llm quota, emit-tool-call allowlist, bounded storage with traversal rejection, log passthrough). Prove the wiring with the probe modes:

```bash
cargo run -p component-harness --release -- \
  target/wasm32-wasip2/release/logic_core_host_example.wasm \
  --probe=full-loop|quota|allowlist|storage
```

- `full-loop` — init ? prepare ? commit asserts `call-llm` was reached (`content="stub-llm-response"`, the host stub's answer, not a guest constant), then `process-tool-result(echo)` asserts `emit-tool-call` executes the allowlisted tool.
- `quota` — 4 commits against one instance; the 4th must surface `permission: llm quota exceeded`.
- `allowlist` — `process-tool-result(tool="rm")` must surface `permission: tool 'rm' not in allowlist`.
- `storage` — a traversal context-id must surface `permission: invalid context id` (both save-state and load-state share the gate).

**Client.** Transpile the host-imports component with jco and map the versioned import to a JS module that implements the same gates (add this `-M` flag alongside the 12 WASI stub mappings from the transpile section below):

```bash
npx jco transpile <logic-core-host>.wasm --out-dir <out> \
  -M "antikythera:agent-sdk/host-imports@1.0.0=./host-imports.js" \
  -M wasi:cli/environment=./wasi-stubs/environment.js \
  ... # the remaining 11 WASI -M flags
```

Pass the mapped module as the `host-imports` entry of the jco import object; its five named exports (`callLlm`, `saveState`, `loadState`, `emitToolCall`, `logMessage`) implement the permission gates, mirroring the server harness.

> **Gate errors must be plain strings, not `Error` objects.** A host function that signals failure with `throw new Error(...)` does not map to the WIT `result<_, string>` error channel in the shape the guest helpers and the harness probes assert; throw a plain string instead (`throw "permission: llm quota exceeded"`), which the guest receives as the `Err(string)` variant. `log-message` has no error channel and must not throw.

### Transpile the composite to browser JS bindings (jco)

The browser path reuses the **composite** WASI component and transpiles it to browser-safe ESM with `@bytecodealliance/jco`. Run `task compose` first (or `task transpile`, which depends on `compose`):

```bash
npx jco transpile dist/antikythera-sdk.wasm --out-dir clients/npm/antikythera-sdk/component \
  -M "antikythera:agent-sdk/runtime-hooks@1.0.0=./runtime-hooks.js" \
  -M wasi:cli/environment=./wasi-stubs/environment.js \
  -M wasi:cli/exit=./wasi-stubs/exit.js \
  -M wasi:cli/stderr=./wasi-stubs/stderr.js \
  -M wasi:cli/stdin=./wasi-stubs/stdin.js \
  -M wasi:cli/stdout=./wasi-stubs/stdout.js \
  -M wasi:clocks/monotonic-clock=./wasi-stubs/monotonic-clock.js \
  -M wasi:clocks/wall-clock=./wasi-stubs/wall-clock.js \
  -M wasi:filesystem/preopens=./wasi-stubs/preopens.js \
  -M wasi:filesystem/types=./wasi-stubs/types.js \
  -M wasi:io/error=./wasi-stubs/error.js \
  -M wasi:io/streams=./wasi-stubs/streams.js \
  -M wasi:random/random=./wasi-stubs/random.js
```

The 13 `-M` flags map the one non-WASI import — `antikythera:agent-sdk/runtime-hooks@1.0.0` — to `runtime-hooks.js` and every WASI import (cli, io, clocks, filesystem, random) to a browser-safe stub under `clients/npm/antikythera-sdk/component/wasi-stubs/`. `runtime-hooks.js` is the **default passthrough stub**: each export returns `{"passthrough": true}` unless a host provider is injected via `globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__` — absence of a provider is never a failure. A `transpile` task is available in `Taskfile.yml` as a shortcut.

Consume from JS as:

```javascript
import { runner } from 'antikythera-agent/component';
const sessionId = runner.init(JSON.stringify({ session_id: 's1' }));
```

See [`WASM_ARCHITECTURE.md`](WASM_ARCHITECTURE.md) for the full `runner` contract and jco pitfalls (jco has no `--dts`; ES2022 target required for top-level await).

### Build and run the server host runtime

`src/antikythera-server-runtime` (Rust crate, lib + bin) embeds the composite via wasmtime and serves the wire protocol over HTTP + SSE (see [`WIRE_PROTOCOL.md`](WIRE_PROTOCOL.md)). Build:

```bash
cargo build -p antikythera-server-runtime
# or
cargo build -p antikythera-server-runtime --release
```

Run with the composite and a stub LLM (no external provider):

```bash
cargo run -p antikythera-server-runtime -- \
  --component dist/antikythera-sdk.wasm \
  --bind 127.0.0.1:8787 \
  --server-tool 'get_current_time:{"datetime":"2026-08-12T00:00:00Z"}' \
  --provider-stub '{"action":"final","content":"smoke-complete"}'
```

Binary flags:

| Flag | Purpose |
|:-----|:--------|
| `--bind <addr>` | HTTP bind address (default `127.0.0.1:8787`) |
| `--component <path>` | Composite component path (default `dist/antikythera-sdk.wasm`) |
| `--client-id <id>` | Server-side client identifier for the SSE control channel |
| `--allow-tool <name>` | Allowlist a tool name (local/remote/mcp destinations) — the gate is default-deny |
| `--server-tool <name>:<response-json>` | Register a deterministic server-owned tool (registration IS a grant; no `--allow-tool` needed) |
| `--provider-stub <json>` | Use a stub LLM provider returning the given response JSON |
| `--smoke` | Run the K1 tool loop against the composite with a stub LLM, then exit |

Verification:

- **Smoke** — `cargo run -p antikythera-server-runtime -- --component dist/antikythera-sdk.wasm --smoke` drives init ? prepare ? LLM stub ? commit and asserts `action=final` (`PASS: smoke — init+prepare+commit with stub LLM reached action=final`).
- **Wire** — start the server, then hit the endpoints: `GET /antikythera/v1/tools` lists the union registry; `POST /antikythera/v1/llm/call` returns the `llm-response` shape; `POST /antikythera/v1/tools/execute` executes a server-owned tool. A gate denial returns HTTP 403 with `{"error": "permission: ..."}`.
- **Tests** — `cargo test -p antikythera-server-runtime` runs the wire-bridge and smoke suites (`tests/runtime_bridge.rs`, `tests/smoke.rs`).

### Use the client host runtime

`clients/npm/antikythera-sdk/runtime/` (exported as `antikythera-agent/runtime`) is the JS host runtime. It wraps the jco-transpiled composite, the server wire protocol, the union registry, permission gates, and the runtime-hooks provider. `createAgentRuntime` selects the core placement:

```javascript
import { createAgentRuntime } from 'antikythera-agent/runtime';

// Client core: owns the runner (jco-transpiled composite) and the tool loop.
const rt = await createAgentRuntime({
  core: 'client',
  serverUrl: 'http://127.0.0.1:8787', // configurable; default 127.0.0.1:8787
  tools: [
    {
      definition: { name: 'my-tool', description: '...' },
      handler: async (args) => ({ success: true, output: JSON.stringify(args) }),
    },
  ],
  policy: { allow: ['my-tool'] }, // default-deny allowlist for local tools
  hooks: {
    // runtime hooks provider — injected into the transpiled runtime-hooks.js
    decideAction: (sessionStateJson, llmResponseJson) =>
      '{"passthrough": true}', // or an override object / throw "permission: ..."
  },
  llm: { model: 'gpt-oss:120b-cloud' },
  maxSteps: 10,
});

await rt.connect();                 // open SSE control channel + init the runner
const result = await rt.runTurn('hello'); // auto tool-owner loop
console.log(result.content);
rt.close();
```

Client-core surface: `runTurn(prompt, opts?)`, `executeTool(name, args?)`, `getState()`, `getToolsPrompt()`, `resetSession()`, `refreshTools()`, `connect()`, `close()`, `onEvent(listener)` (receives `llm-token`, `tool_requested`, `tool_result`, `error`, ...). Server-core mode (`core: 'server'`) is a control-channel peer without the runner: it answers SSE `tool-execution-request` / `hook-request` events via POST-back and exposes `executeLocalTool(name, args?)`.

Reference flow: `examples/antikythera-web` (`src/slices/chat/flow/send-message.ts`) uses exactly this surface — `VITE_SERVER_URL` / `localStorage['antikythera_server_url']` select the server URL, `onEvent` drives streaming and tool progress, `getState()` enriches the final tool records.

## Docs site build

The repository also includes an `mdBook` configuration that turns `README.md` plus the `documentation/` folder into a static documentation site.

```mermaid
flowchart LR
    README[README.md] --> SUMMARY[SUMMARY.md]
    DOCS[documentation/*.md] --> SUMMARY
    SUMMARY --> MDBOOK[mdbook build]
    MDBOOK --> SITE[book/]
    SITE --> PAGES[GitHub Pages deployment]
```

### Local commands

```bash
# Build static site
mdbook build

# Preview locally
mdbook serve --open
```

## Tests and quality checks

### Verification flow

```mermaid
flowchart LR
    CHECK[cargo check --workspace] --> TEST[cargo test --workspace]
    TEST --> FMT[cargo fmt --all -- --check]
    FMT --> CLIPPY[cargo clippy --workspace --lib --bins -- -D warnings -D deprecated]
```

### Workspace-wide

```bash
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --lib --bins -- -D warnings -D deprecated
```

### Common targeted checks

```bash
# SDK library tests
cargo test -p antikythera-sdk --lib

# Check all workspace crates without producing binaries
cargo check --workspace
```

## Taskfile helpers

The repository includes `Taskfile.yml` for common flows.

| Task | Purpose |
|:-----|:--------|
| `task build` | Build the composite WASM component (SDK + toolrunner + default-hooks + compose) |
| `task build-wasm-harness` | Build the standalone SDK WASM artifact (intermediate) |
| `task build-toolrunner` | Build the standalone toolrunner WASM artifact (intermediate) |
| `task compose` | Compose SDK + toolrunner + default-hooks into `dist/antikythera-sdk.wasm` |
| `task compose-hooks-custom` | Compose SDK + toolrunner with a host-authored logic-hooks component in place of default-hooks |
| `task build-all` | Build all WASM artifacts (standalone SDK + toolrunner + default-hooks + composite) |
| `task wit` | Validate WIT conformance |
| `task transpile` | Transpile the **composite** to JS bindings with jco (depends on `compose`; 13 `-M` mappings — `runtime-hooks` + 12 WASI stubs) |
| `task transpile-clean` | Remove transpiled JS output directory |
| `task test` | Run workspace tests |
| `task test-sdk` | Run SDK tests |
| `task test-ffi` | Run FFI tests |
| `task test-component` | Run component tests |
| `task check` | Run `cargo check --workspace` |
| `task check-wasm` | Check WASM compilation |
| `task lint` | Run Clippy |
| `task format` | Run rustfmt |
| `task inspect` | Inspect the built component with `wasm-tools` if installed |
| `task size` | Show binary sizes |
| `task clean` | Clean all build artifacts |
| `task clean-wasm` | Clean WASM build artifacts |

## Notes

- The composite build (`wasm32-wasip2`, feature `component` on all three crates, then `wasm-tools compose`) is the WASM deployment target for both server and browser. Use `dist/antikythera-sdk.wasm` when embedding agent logic in a host application via wasmtime (`src/examples/component-harness` proves the builtin tool path and the runtime-hooks probes), and transpile the composite with jco for browser consumption. For full client–server connectivity, use the two host runtimes -- `src/antikythera-server-runtime` (Rust, HTTP + SSE) and `clients/npm/antikythera-sdk/runtime` (`antikythera-agent/runtime`) -- see the Runtime Bridge sections above and [`WIRE_PROTOCOL.md`](WIRE_PROTOCOL.md).
- The standalone `src/antikythera-sdk` component is an intermediate artifact: it imports `tool-registry`, `logic-hooks`, and `runtime-hooks` and must be composed with `antikythera-toolrunner` and a hooks provider (default: `antikythera-default-hooks`) before any consumer (wasmtime harness, jco) touches it. After composition the `runtime-hooks` import remains and the host MUST wire it (default passthrough stub) -- see [`WASM_ARCHITECTURE.md`](WASM_ARCHITECTURE.md) -- Runtime hooks (host-supplied).
