# Browser WASM Contract

This directory defines the contract for the **browser/WASM target** of the SDK: a WASI component (`wasm32-wasip2`) transpiled to JavaScript bindings with `jco`.

## Contract Files

- `browser_function_signatures.golden.txt` — expected exported function signatures (golden file)
- `../shared/wit_signatures.golden.txt` — WIT signatures for the `runner` interface (reference)
- `../shared/payload_contract.golden.json` — shared JSON payload key shapes (reference)

The **source of truth for the transpiled output** is the generated TypeScript artifact:
`clients/npm/antikythera-sdk/component/interfaces/antikythera-agent-sdk-runner.d.ts`
(plus the world root `clients/npm/antikythera-sdk/component/antikythera-sdk.d.ts`).

## Build Target

- Component target: `wasm32-wasip2` (WASI Component Model)
- Feature flag: `component`
- Build command:

```bash
cargo component build -p antikythera-sdk --release --target wasm32-wasip2 --no-default-features --features component
```

- Transpile to JS bindings (jco emits TypeScript by default; there is no `--dts` flag — use `--no-typescript` only to suppress it):

```bash
npx jco transpile dist/antikythera-sdk.wasm --out-dir clients/npm/antikythera-sdk/component \
  -M "wasi:cli/environment@0.2.3=./wasi-stubs/environment.js" \
  -M "wasi:cli/exit@0.2.3=./wasi-stubs/exit.js" \
  -M "wasi:cli/stderr@0.2.3=./wasi-stubs/stderr.js" \
  -M "wasi:cli/stdin@0.2.3=./wasi-stubs/stdin.js" \
  -M "wasi:cli/stdout@0.2.3=./wasi-stubs/stdout.js" \
  -M "wasi:clocks/monotonic-clock@0.2.3=./wasi-stubs/monotonic-clock.js" \
  -M "wasi:clocks/wall-clock@0.2.3=./wasi-stubs/wall-clock.js" \
  -M "wasi:filesystem/preopens@0.2.3=./wasi-stubs/preopens.js" \
  -M "wasi:filesystem/types@0.2.3=./wasi-stubs/types.js" \
  -M "wasi:io/error@0.2.3=./wasi-stubs/error.js" \
  -M "wasi:io/streams@0.2.3=./wasi-stubs/streams.js" \
  -M "wasi:random/random@0.2.3=./wasi-stubs/random.js"
```

The 12 `-M` flags map every WASI import the component makes (cli, io, clocks, filesystem, random) to the browser-safe stubs under `clients/npm/antikythera-sdk/component/wasi-stubs/`. Command details live in `documentation/BUILD.md` and the `transpile` task in `Taskfile.yml`.

## Interface

The component exports the `runner` interface (world `antikythera-agent-sdk`, defined in `wit/antikythera.wit`). After jco transpilation the functions are exposed as **camelCase** functions on the `runner` namespace. All payloads are JSON strings passed as `string` arguments; WIT `option<T>` renders as `T | undefined`; WIT `result<T, string>` errors surface on the JS side as thrown errors.

### Exported Functions (runner)

| Function | Parameters | Return |
|----------|-----------|--------|
| `init` | `configJson: string` | `string` |
| `prepareUserTurn` | `requestJson: string` | `string` |
| `commitLlmResponse` | `preparedTurnJson: string, llmResponseJson: string` | `string` |
| `commitLlmStream` | `preparedTurnJson: string` | `string` |
| `processLlmResponseForSession` | `sessionId: string, llmResponseJson: string` | `string` |
| `processToolResultForSession` | `sessionId: string, toolResultJson: string` | `string` |
| `appendLlmChunk` | `sessionId: string, chunk: string, correlationId: string \| undefined` | `boolean` |
| `drainEvents` | `sessionId: string` | `string` |
| `getState` | `sessionId: string` | `string` |
| `resetSession` | `sessionId: string` | `boolean` |
| `sweepIdleSessions` | `nowUnixMs: bigint \| undefined` | `number` |
| `registerTools` | `toolsJson: string` | `number` |
| `getToolsPrompt` | — | `string` |
| `setContextPolicy` | `policyJson: string` | `boolean` |
| `getTelemetrySnapshot` | `sessionId: string` | `string` |
| `getSloSnapshot` | `sessionId: string` | `string` |

## Verification

```bash
cargo test -p antikythera-tests --test compatibility_tests
```

The `browser_type_signatures_match_golden` test asserts the golden signatures match the generated `antikythera-agent-sdk-runner.d.ts` bidirectionally (every golden name/signature exists in the d.ts, and every d.ts runner export is listed in the golden).
