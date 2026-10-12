# Code hygiene

The cleanup removes implementation that has no active caller or build path and
places behavioral explanations on the declarations they describe.

## Removed implementation

- Eleven Rust source files were outside every module tree rooted at the 81 Cargo
  targets: the old discovery and terminal modules, the old model-provider port
  facade, and two redundant content facades. The remaining Rust source tree has
  no unresolved file-backed modules or orphan source files.
- OpenAI and Ollama clients no longer store an unread copy of the constructor's
  model string. The model still contributes to the client identifier, and each
  request retains its explicit model selection.
- Empty glob re-exports from the client's implementation modules were removed.
  Their methods remain available on `McpClient`.
- JSON-RPC result extraction no longer repeats a condition after that condition
  has already returned. Retry execution returns the final failure directly from
  the loop instead of initializing an overwritten optional value and retaining
  an unreachable default error.
- The unused Node loader probe and six unused core dependency declarations were
  removed. Version synchronization validates every target; the temporary path
  for missing version markers is no longer needed.
- Native dotenv initialization is compiled only for native targets. WASM builds
  do not carry an unused environment-loader static.

## Documentation conventions

Rust uses `///` for items and `//!` for modules. JavaScript and TypeScript use
JSDoc blocks. Python uses module, class, and function docstrings, plus `#:` for
attribute documentation. WIT uses `///` descriptions. Explanations describe
behavior, input and output contracts, ownership, failure handling, and target
restrictions; section dividers and comments that repeat the code are removed.

Compiler and lint directives remain executable tooling instructions. Generated
wit-bindgen and jco bindings retain their generator output; handwritten adapters
and WASI stubs follow the documentation conventions above.

## Verification

- The scan covers 780 handwritten source, test, example, and template files.
  It finds no ordinary comments outside documentation and the 73 retained
  compiler/lint directives, and no unit or integration test sources outside
  the root `tests/` directory.
- Rust: 821 passed, 0 failed, and 17 existing ignored documentation examples.
- Node: 97 passed with no skips. Python: 361 passed.
- Clippy with warnings denied, Ruff, Mypy, and workspace rustdoc pass. Rustdoc
  reports no broken links, malformed code blocks, or invalid HTML tags.
- The WASI p2 SDK, toolrunner, and default hooks were rebuilt and composed. jco
  regenerated both the shared namespace bundle and the per-instance factory.
  The packaging gate verifies the npm and Python bundle bytes match and that
  every published package target resolves.
- The composite in `dist/` and the Python package have the same SHA-256:
  `d782905b681ca560187f2b54f1910b348686e9eb16c2b8e35b88045a42137032`.
