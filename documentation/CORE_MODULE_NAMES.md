# Descriptive names for client core modules

`node src/scripts/transpile-client.mjs` transpiles the composed WASM Component
Model artifact with jco, then assigns role names to every emitted core module.
The namespace bundle has six modules; the isolated async factory currently has
thirteen. Both bundles are copied into the Python distribution with identical
filenames and bytes. The composed artifact remains `dist/antikythera-sdk.wasm`.

All filenames below use the prefix `antikythera-sdk.` and suffix `.core.wasm`.
The original names describe the current factory output before postprocessing;
their numbers are not part of the naming contract.

| Original factory name | Descriptive name between prefix and suffix | Responsibility |
| --- | --- | --- |
| `core.wasm` | `runner` | SDK sessions, turns, tool dispatch, and runner WIT exports. |
| `core2.wasm` | `runner.wasi-preview1-adapter` | Adapts the runner's Preview 1 imports to WASI Preview 2, including clocks, randomness, streams, and environment access. |
| `core3.wasm` | `runner.import-trampolines` | Supplies indirect import functions while the runner and its dependencies are instantiated. |
| `core4.wasm` | `runner.import-fixups` | Populates the runner trampoline table with the resolved imports. |
| `core5.wasm` | `tool-registry` | Tool registry, validation, and built-in tool execution. |
| `core6.wasm` | `tool-registry.wasi-preview1-adapter` | Adapts toolrunner imports, including randomness, streams, and environment access. |
| `core7.wasm` | `tool-registry.import-trampolines` | Supplies toolrunner indirect import functions during instantiation. |
| `core8.wasm` | `tool-registry.import-fixups` | Populates the toolrunner trampoline table with resolved imports. |
| `core9.wasm` | `logic-hooks-passthrough` | Default hook implementation preserving SDK decisions. |
| `core10.wasm` | `logic-hooks-passthrough.wasi-preview1-adapter` | Adapts default-hook imports, including streams and environment access. |
| `core11.wasm` | `logic-hooks-passthrough.import-trampolines` | Supplies default-hook indirect import functions during instantiation. |
| `core12.wasm` | `logic-hooks-passthrough.import-fixups` | Populates the default-hook trampoline table with resolved imports. |
| `core13.wasm` | `component-abi-adapters` | Connects composed component calls through canonical ABI adapters, including memory allocation, string transcoding, and post-return handling. |

The namespace bundle contains the three application modules and their three
WASI adapters. Its previous `wasi-clocks`, `wasi-streams`, and `wasi-filesystem`
names are replaced with the owning component plus `wasi-preview1-adapter`, since
each adapter handles multiple WASI interfaces. Consumers loading individual core
files by a hard-coded path must update to these names. Public runner imports and
the `component/instantiate` entry point continue to use the generated loaders.

## Classification and failure behavior

`src/scripts/post-transpile-rename.mjs` validates binaries and uses WIT exports,
WASI exports, and wit-component debug names to classify modules. Import-table
fixups are matched to trampolines by their type sections and numbered function
surfaces. It never derives a role from the original core filename or file order.
The helper modules implement the shim and fixup structure described in the
[wit-component encoder](https://github.com/bytecodealliance/wasm-tools/blob/main/crates/wit-component/src/encoding.rs).

The script rewrites both URL literals in namespace bindings and the filename
arguments to the factory's `getCoreModule` callback. jco's explicit instantiation
API is documented in its [transpilation guide](https://bytecodealliance.github.io/jco/transpiling.html).
The binary contents are preserved. Repeating the rename makes no changes.

Unknown roles, ambiguous fixup matches, target collisions, and missing loader
references stop the build before files are renamed. If the component shape or
jco output changes, update classification and its tests before shipping. Keep
wit-component name sections through transpilation because trampoline ownership
uses those names; stripping them requires a different ownership classifier.

## Verification

Run `node --test tests/node/core-module-names.test.mjs`. The suite uses real
generated binaries and loaders, reverses opaque numbering, checks byte identity
and repeatability, and verifies that rejected bundles remain unchanged. It also
checks that npm and Python expose the same module inventory and loader bytes.
CI transpiles through the shared script, and the packaging gate checks that
every referenced core module is included in the npm tarball and Python wheel.
