# Antikythera Agent SDK

Antikythera Agent SDK is a Rust workspace for building MCP-capable agent runtimes, host-integrated orchestration flows, and portable WASM agent components.

Runtime consistency, Component Model ownership, client setup, and validation
are documented in [Runtime consistency](documentation/RUNTIME_CONSISTENCY.md).
The client uses a per-instance jco factory; Python embeds the same composite
WASM used to generate the client factory.

## Packages

| Platform | Package | Link |
|:---------|:--------|:-----|
| **npm** | `antikythera-agent` | [npmjs.com/package/antikythera-agent](https://www.npmjs.com/package/antikythera-agent) |
| **PyPI** | `antikythera-agent` | [pypi.org/project/antikythera-agent](https://pypi.org/project/antikythera-agent/) |

## System Overview

```mermaid
flowchart TD
    Host[Host Application] --> Facade[antikythera-facade]
    Host --> SDK[antikythera-sdk]
    Facade --> Core[antikythera-core]
    SDK --> Core
    Core --> Domain[antikythera-domain]
    Core --> Ports[antikythera-ports]
    Core --> Config[antikythera-config]
    Core --> Session[antikythera-session]
    Core --> Log[antikythera-log]
    Core --> Resilience[antikythera-resilience]
    Core --> Tooling[antikythera-tooling]
    Core --> Observability[antikythera-observability]
    Core --> Security[antikythera-security]
    Facade --> Providers[LLM Providers]
    Providers --> Ollama[antikythera-provider-ollama]
    Providers --> OpenAI[antikythera-provider-openai]
    Providers --> Gemini[antikythera-provider-gemini]
```

## What Is Included

- Modular workspace crates organized in 6 layers (types ? implementations ? core ? providers ? SDK/facade ? deployment).
- Multi-provider LLM support: Ollama (default), OpenAI, and Gemini via feature-gated facade.
- Multi-agent orchestration with guardrails, resilience, and observability hooks.
- Streaming support for token/event output and buffered delivery policies.
- WASM component integration path for host-controlled execution (WASI Component Model `wasm32-wasip2` on the server and browser via `jco`), plus a **Runtime Bridge** (Rust server host + JS client host, HTTP + SSE wire protocol): the core side (client or server) is selected when the runtime is created (`createAgentRuntime({ core: 'client' | 'server' })`), not migrated live, and a running server is required for execution (LLM proxy + SSE control channel).
- Python SDK bridge (parity 7/7 wire-protocol, E2E jco 3/3): `antikythera_agent.server` (`createAgentServer`, `AgentServer`) is a drop-in peer of the Rust server and serves the jco bundle at `GET /antikythera/v1/component/{path}`; JS clients load it via `createAgentRuntime({ serverUrl, componentBase })` (manifest `GET /antikythera/v1/component/manifest`).
- Consolidated documentation under `documentation/`.

## Workspace Layout

### Layer 0: Types

| Crate | Role |
|:------|:-----|
| `src/antikythera-domain` | Canonical domain types (entities, sessions, FSM, validation) — zero internal deps |
| `src/antikythera-ports` | Port trait definitions (hexagonal architecture) — depends on domain |

### Layer 1: Implementations

| Crate | Role |
|:------|:-----|
| `src/antikythera-config` | Configuration schema and loading — depends on domain |
| `src/antikythera-log` | Structured logging and subscriptions — standalone |
| `src/antikythera-resilience` | Retry, timeout, context window, and health tracking — depends on domain |
| `src/antikythera-tooling` | MCP tool server management — depends on domain, config |
| `src/antikythera-observability` | In-memory observability implementations — depends on domain, ports |
| `src/antikythera-security` | Validation, rate limiting, secrets management — depends on domain, ports |
| `src/antikythera-storage` | Session persistence with pluggable backends — depends on domain |

### Layer 2: Core

| Crate | Role |
|:------|:-----|
| `src/antikythera-core` | Core MCP runtime, agent logic, transports — depends on domain, ports, config, log, resilience, tooling, observability |
| `src/antikythera-session` | Session management with persistent chat history — depends on domain, log |

### Layer 3: Providers

| Crate | Role |
|:------|:-----|
| `src/antikythera-provider-ollama` | Ollama LLM provider — depends on domain, core |
| `src/antikythera-provider-openai` | OpenAI LLM provider — depends on domain, core |
| `src/antikythera-provider-gemini` | Google Gemini LLM provider — depends on domain, core |

### Layer 4: SDK & Facade

| Crate | Role |
|:------|:-----|
| `src/antikythera-sdk` | Public API layer for Rust and WASM component bindings — depends on core, log, session |
| `src/antikythera-facade` | High-level API with provider selection (Ollama/OpenAI/Gemini) — depends on core, domain, log |

### Layer 5: Deployment

| Crate | Role |
|:------|:-----|
| `src/plugin/antikythera-toolrunner` | In-process tool execution and standalone WASM tool-registry component (composed into the SDK deliverable) |
| `src/plugin/antikythera-default-hooks` | Default no-op logic-hooks implementation (composed into the SDK deliverable) |
| `src/antikythera-server-runtime` | Server host runtime for the WASM composite — wasmtime core + HTTP/SSE wire bridge (lib + bin) |

### Supporting directories

| Path | Role |
|:-----|:-----|
| `tests/` | Workspace integration tests and scenario coverage |
| `src/scripts/` | WIT generation and component build helpers |
| `documentation/` | Focused guides and references |

### Examples

| Path | Role |
|:-----|:-----|
| `src/examples/chat` | Rust chat example using antikythera-facade |
| `examples/antikythera-web` | TypeScript/Vite web frontend using the client host runtime (`antikythera-agent/runtime`) (not a workspace member) |
| `src/examples/component-harness` | Wasmtime server proof: composite verification, logic-hooks/logic-core/host-imports probes, runtime-hooks probes |
| `src/examples/logic-hooks-template` | Host-authored logic-hooks component template (composed in place of default-hooks) |
| `examples/logic-hooks-example` | Filled-in logic-hooks probe (`decide-action` forces `action=final`) |
| `src/examples/logic-core-template` | Drop-in runner template (world logic-core-component) |
| `examples/logic-core-example` | Deterministic drop-in logic core (echo-agent) + swap proof |
| `examples/logic-core-host-example` | Full custom loop via host-imports (call-llm/save/load/emit-tool-call/log) + permission-gated host implementations |
| `clients/python/antikythera_agent/server` | Python Runtime Bridge server (drop-in wire-protocol peer + jco component delivery via `GET /antikythera/v1/component/{path}`) |

## Build and Validate

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --lib --bins -- -D warnings -D deprecated
```

## Documentation Index

### Framework

- [Architecture](documentation/ARCHITECTURE.md) — crate relationships and request flow
- [Workspace](documentation/WORKSPACE.md) — repository organization and crate responsibilities
- [Product Scope](documentation/PRODUCT_SCOPE.md) — deployment targets and feature flags
- [Build](documentation/BUILD.md) — build commands for each target
- [Config](documentation/CONFIG.md) — configuration format and loading
- [Component](documentation/COMPONENT.md) — WASM component model details

### Runtime

- [Servers and Agents](documentation/SERVERS_AND_AGENTS.md) — server and agent management
- [Streaming](documentation/STREAMING.md) — token/event streaming behavior
- [Resilience](documentation/RESILIENCE.md) — retry, timeout, and health tracking
- [Guardrails](documentation/GUARDRAILS.md) — runtime hardening controls
- [Hooks](documentation/HOOKS.md) — host integration hooks
- [Context Management](documentation/CONTEXT_MANAGEMENT.md) — context window management

### Reference

- [Logging](documentation/LOGGING.md) — structured logging system
- [Security](documentation/SECURITY.md) — input validation, rate limiting, secrets
- [Storage](documentation/STORAGE.md) — pluggable session persistence
- [Cache](documentation/CACHE.md) — caching layer details
- [Import Export](documentation/IMPORT_EXPORT.md) — backup and restore flows
- [JSON Schema](documentation/JSON_SCHEMA.md) — schema definitions
- [MCP Contracts](documentation/MCP_CONTRACTS.md) — MCP protocol contracts
- [Observability](documentation/OBSERVABILITY.md) — metrics and telemetry

### WASM

- [WASM Agent](documentation/WASM_AGENT.md) — agent logic inside the component
- [WASM Architecture](documentation/WASM_ARCHITECTURE.md) — WASM deployment architecture
- [Wire Protocol](documentation/WIRE_PROTOCOL.md) — Runtime Bridge HTTP + SSE wire contract
- [Runtime Bridge Decisions](documentation/DECISIONS_RUNTIME_BRIDGE.md) — Runtime Bridge design decision register

### Process

- [Testing](documentation/TESTING.md) — test strategy and commands
- [Migration](documentation/MIGRATION.md) — documentation structure history
- [Deprecation Policy](documentation/DEPRECATION_POLICY.md) — deprecation lifecycle
