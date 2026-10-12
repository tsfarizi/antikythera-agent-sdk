# Test layout

All unit and integration test sources, fixtures, test-only helpers, and browser checks
live under this directory.

- `rust/`: the `antikythera-tests` workspace package and its integration suites.
- `rust/unit/`: Rust unit test modules, loaded by their owning production
  module through `#[cfg(test)]` and `#[path]`. Module names and access to private
  implementation details are preserved.
- `rust/server-runtime/`: the server-runtime bridge and smoke integration tests.
- `rust/toolrunner/`: the toolrunner plugin integration test.
- `node/`: Node test suites, browser checks, loader helpers, and Bun shims.
- `python/`: Python tests. Root `pytest.ini` supplies the local package path.
- `packaging/`: the npm/Python publication gate.

Run from the repository root:

```sh
# Requires the component build artifacts and wasm-tools.
cargo test --workspace

node --test --test-concurrency=1 tests/node/*.test.mjs
node tests/node/component-smoke.mjs
python -m pytest
node tests/packaging/pack-smoke.mjs
npm --prefix clients run test:types

ruff check clients/python/antikythera_agent tests/python
```

The npm package's `npm test` command points to the focused suites here.
CI and release workflows use the same test paths.

`node/core-module-names.test.mjs` verifies descriptive WASM filenames in both
jco bundles, npm/Python byte parity, renumbering independence, repeatability,
and rejection of unknown or inconsistent bundles without partial changes.

Configured-agent delivery is covered by `node/delivery.test.mjs`,
`python/test_delivery.py`, and the compiler-checked `typescript/delivery.ts`.
The Node acceptance suite also starts a Python host and executes its exported
agent through real jco bindings. Build the component bundle first and install
the Python WASM/test extras before running these integration suites.

For browser delivery acceptance, run
`node tests/node/helpers/delivery-browser-host.mjs` and open its printed URL.
The browser fixture must show `PASS`, two steps, and a tool result whose
`execution` field is `browser`.

For the browser check, serve the repository root with a local HTTP server,
then open `/tests/node/browser-component-smoke.html`. Its component imports
resolve to the generated bundle under `clients/npm/antikythera-sdk/component/`.

Public API examples remain in Rust documentation and continue to run as
doctests through Cargo. They are documentation examples, separate from the
unit and integration modules organized here.
