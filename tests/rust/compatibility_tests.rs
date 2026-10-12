//! Contract compatibility tests — mechanical verification of golden artifacts.
//!
//! These tests are deliberately **text-only**: they read the golden files under
//! `contracts/` and the real transpiled TypeScript output under `clients/npm/` and
//! assert bidirectional agreement. No WASM runtime, no browser.
//!
//! Contract under test (per `contracts/browser/README.md`):
//! the WASI component (`wasm32-wasip2`) transpiled by jco exposes the `runner`
//! namespace with 16 camelCase functions whose signatures must match
//! `clients/npm/antikythera-sdk/component/interfaces/antikythera-agent-sdk-runner.d.ts`.
//!
//! Composite-artifact test (`composed_component_world_single_runtime_hooks_import`)
//! shells out to `wasm-tools component wit` on the composite
//! `dist/antikythera-sdk.wasm` and asserts the composed world imports only
//! `wasi:` interfaces plus the single host-wired
//! `antikythera:agent-sdk/runtime-hooks@1.0.0` import, and exports the pinned
//! runner id. This test SKIPs (not fails) when the build artifact or the tool
//! is absent — the composite is a build artifact, not a source unit.
//!
//! Falsification: if the transpiled d.ts drifts (renamed function, changed
//! parameter/return type, added/removed export) without the golden being
//! updated, these tests go RED.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Resolves the repository root.
///
/// The `antikythera-tests` package lives at `<root>/tests/rust`, so the manifest
/// directory is two levels below the repository root.
/// `CARGO_MANIFEST_DIR` is read at runtime with the compile-time baked value
/// as fallback; existence of `contracts/` and `clients/npm/` is asserted so a wrong
/// resolution fails loudly instead of producing a confusing file-not-found.
fn repo_root() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));

    let root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("antikythera-tests package must live at <root>/tests/rust")
        .to_path_buf();

    assert!(
        root.join("contracts").is_dir(),
        "repo root must contain contracts/ (resolved root: {})",
        root.display()
    );
    assert!(
        root.join("clients/npm").is_dir(),
        "repo root must contain clients/npm/ (resolved root: {})",
        root.display()
    );
    root
}

fn read_repo_file(root: &Path, rel: &str) -> String {
    let path = root.join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read repo file {rel} (resolved: {}): {e}",
            path.display()
        )
    })
}

/// Locates the `wasm-tools` executable for the composition test.
///
/// Resolution order (deterministic, first hit wins):
/// 1. `WASM_TOOLS` environment variable (explicit override),
/// 2. `PATH` entries,
/// 3. `$CARGO_HOME/bin`,
/// 4. `$HOME/.cargo/bin` (falling back to `$USERPROFILE/.cargo/bin` on Windows).
///
/// Returns `None` when no candidate is an existing file; the caller turns
/// that into a SKIP, never a failure — the composite is a build artifact and
/// wasm-tools is a build tool, neither of which belongs to the source unit.
fn find_wasm_tools() -> Option<PathBuf> {
    if let Some(override_path) = std::env::var_os("WASM_TOOLS") {
        let path = PathBuf::from(override_path);
        if path.is_file() {
            return Some(path);
        }
    }

    let exe_name = if cfg!(windows) {
        "wasm-tools.exe"
    } else {
        "wasm-tools"
    };
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            candidates.push(dir.join(exe_name));
        }
    }
    if let Some(cargo_home) = std::env::var_os("CARGO_HOME") {
        candidates.push(PathBuf::from(cargo_home).join("bin").join(exe_name));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if let Some(home) = home {
        candidates.push(
            PathBuf::from(home)
                .join(".cargo")
                .join("bin")
                .join(exe_name),
        );
    }

    candidates.into_iter().find(|p| p.is_file())
}

const GOLDEN_BROWSER_SIGS: &str = "contracts/browser/browser_function_signatures.golden.txt";
const DTS_RUNNER_INTERFACE: &str =
    "clients/npm/antikythera-sdk/component/interfaces/antikythera-agent-sdk-runner.d.ts";
const DTS_SDK_ROOT: &str = "clients/npm/antikythera-sdk/component/antikythera-sdk.d.ts";
const GOLDEN_PAYLOAD_CONTRACT: &str = "contracts/shared/payload_contract.golden.json";

const EXPECTED_RUNNER_FUNCTION_COUNT: usize = 16;

/// Logic-hooks contract (interface `logic-hooks` in `wit/antikythera.wit`).
///
const WIT_ANTIKYTHERA: &str = "wit/antikythera.wit";
const GOLDEN_WIT_SIGS: &str = "contracts/shared/wit_signatures.golden.txt";
const SDK_BINDINGS_REL: &str = "src/antikythera-sdk/src/bindings.rs";

/// Bare signatures of the three logic-hooks functions, exactly as declared in
/// `wit/antikythera.wit` (interface `logic-hooks`), in WIT definition order.
const EXPECTED_LOGIC_HOOK_SIGNATURES: [&str; 3] = [
    "prepare-turn: func(request-json: string, session-state-json: string) -> result<string, string>",
    "decide-action: func(session-state-json: string, llm-response-json: string) -> result<string, string>",
    "handle-tool-result: func(session-state-json: string, tool-result-json: string) -> result<string, string>",
];

/// Snake-case Rust binding names for the three logic-hooks functions
/// (kebab-case WIT function names are transpiled by wit-bindgen).
const EXPECTED_LOGIC_HOOK_FUNCTIONS: [&str; 3] =
    ["prepare_turn", "decide_action", "handle_tool_result"];

const GOLDEN_LOGIC_HOOKS_BLOCK: &str = "logic-hooks {";
const GOLDEN_LOGIC_HOOKS_COMPONENT_WORLD: &str =
    "world logic-hooks-component { export logic-hooks; }";
const GOLDEN_SDK_WORLD_WITH_HOOKS: &str = "world antikythera-agent-sdk { import tool-registry; import logic-hooks; import runtime-hooks; export runner; }";

/// Bare signatures of the three runtime-hooks functions, exactly as declared
/// in `wit/antikythera.wit` (interface `runtime-hooks`), in WIT definition
/// order. Identical to the logic-hooks signatures: `runtime-hooks` is the
/// runtime-wired twin of the composed `logic-hooks` interface — the same
/// three decision functions with the same JSON-object semantics, lower
/// precedence (A1a: the composed provider is consulted first, `runtime-hooks`
/// runs only on passthrough).
const EXPECTED_RUNTIME_HOOK_SIGNATURES: [&str; 3] = [
    "prepare-turn: func(request-json: string, session-state-json: string) -> result<string, string>",
    "decide-action: func(session-state-json: string, llm-response-json: string) -> result<string, string>",
    "handle-tool-result: func(session-state-json: string, tool-result-json: string) -> result<string, string>",
];

/// Snake-case Rust binding names for the three runtime-hooks functions
/// (kebab-case WIT function names are transpiled by wit-bindgen).
const EXPECTED_RUNTIME_HOOK_FUNCTIONS: [&str; 3] =
    ["prepare_turn", "decide_action", "handle_tool_result"];

const GOLDEN_RUNTIME_HOOKS_BLOCK: &str = "runtime-hooks {";

/// Logic-core drop-in contract (world `logic-core-component` in `wit/antikythera.wit`). A custom
/// logic core exports a `runner` interface IDENTICAL to the SDK runner — the same 16 functions —
/// so the host can swap the loaded component without any code change. The imports are optional
/// and pruned when unused by the component encoder.
///
const GOLDEN_LOGIC_CORE_COMPONENT_WORLD: &str =
    "world logic-core-component { import host-imports; import tool-registry; export runner; }";
const WIT_LOGIC_CORE_WORLD_MARKER: &str = "world logic-core-component {";
const WIT_LOGIC_CORE_MEMBERS: [&str; 3] = [
    "import host-imports;",
    "import tool-registry;",
    "export runner;",
];
const LOGIC_CORE_EXAMPLE_WASM_REL: &str = "target/wasm32-wasip2/release/logic_core_example.wasm";
const LOGIC_CORE_HOST_EXAMPLE_WASM_REL: &str =
    "target/wasm32-wasip2/release/logic_core_host_example.wasm";
const HOST_IMPORTS_IMPORT: &str = "import antikythera:agent-sdk/host-imports@1.0.0";
const VOCABULARY_IMPORT: &str = "import antikythera:agent-sdk/vocabulary@1.0.0";
const LOGIC_CORE_RUNNER_EXPORT: &str = "export antikythera:agent-sdk/runner@1.0.0";
const LOGIC_CORE_RUNNER_FUNCTIONS: [&str; 16] = [
    "init",
    "prepare-user-turn",
    "commit-llm-response",
    "commit-llm-stream",
    "process-llm-response-for-session",
    "process-tool-result-for-session",
    "append-llm-chunk",
    "drain-events",
    "get-state",
    "reset-session",
    "sweep-idle-sessions",
    "register-tools",
    "get-tools-prompt",
    "set-context-policy",
    "get-telemetry-snapshot",
    "get-slo-snapshot",
];

/// Standalone SDK component contract (uncomposed build artifact).
///
const STANDALONE_SDK_WASM_REL: &str = "target/wasm32-wasip2/release/antikythera_sdk.wasm";
const STANDALONE_IMPORT_TOOL_REGISTRY: &str = "import antikythera:agent-sdk/tool-registry@1.0.0";
const STANDALONE_IMPORT_LOGIC_HOOKS: &str = "import antikythera:agent-sdk/logic-hooks@1.0.0";
const STANDALONE_IMPORT_RUNTIME_HOOKS: &str = "import antikythera:agent-sdk/runtime-hooks@1.0.0";
const STANDALONE_EXPORT_RUNNER: &str = "export antikythera:agent-sdk/runner@1.0.0";

const COMPOSED_WASM_REL: &str = "dist/antikythera-sdk.wasm";
const COMPOSED_EXPECTED_RUNNER_EXPORT: &str = "export antikythera:agent-sdk/runner@1.0.0";
const COMPOSED_EXPECTED_RUNTIME_HOOKS_IMPORT: &str =
    "import antikythera:agent-sdk/runtime-hooks@1.0.0";
const COMPOSED_IMPORT_PREFIX: &str = "import wasi:";

/// Golden lines: non-blank, non-`#`-comment lines of the form
/// `name(param: type, ...): return`.
fn parse_golden_signatures(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// jco d.ts export lines: `export function name(params): ret;` → strip the
/// `export function ` prefix and the trailing `;` to obtain the bare signature.
fn parse_dts_signatures(content: &str) -> Vec<String> {
    let mut signatures = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("export function ") {
            let rest = rest.strip_suffix(';').unwrap_or(rest).trim();
            signatures.push(rest.to_string());
        }
    }
    signatures
}

/// Function name from a signature line `name(params): ret`.
fn signature_name(signature: &str) -> &str {
    signature
        .split('(')
        .next()
        .expect("signature must contain '(' before the parameter list")
        .trim()
}

#[test]
/// 1) Every golden function must exist in the transpiled d.ts.
///
/// 2) Every runner export in the d.ts must be listed in the golden.
///
/// 3) Full signature equality — catches parameter/return type drift (e.g. a jco re-render
/// changing `bigint | undefined` or a renamed parameter).
///
/// 4) Count guard — the runner interface is fixed at 16 functions.
///
fn browser_type_signatures_match_golden() {
    let root = repo_root();
    let golden_content = read_repo_file(&root, GOLDEN_BROWSER_SIGS);
    let dts_content = read_repo_file(&root, DTS_RUNNER_INTERFACE);

    let golden = parse_golden_signatures(&golden_content);
    let dts = parse_dts_signatures(&dts_content);

    let golden_names: BTreeSet<&str> = golden.iter().map(|s| signature_name(s)).collect();
    let dts_names: BTreeSet<&str> = dts.iter().map(|s| signature_name(s)).collect();

    let missing_in_dts: Vec<&&str> = golden_names.difference(&dts_names).collect();
    assert!(
        missing_in_dts.is_empty(),
        "golden functions missing from {DTS_RUNNER_INTERFACE}: {missing_in_dts:?}"
    );

    let extra_in_dts: Vec<&&str> = dts_names.difference(&golden_names).collect();
    assert!(
        extra_in_dts.is_empty(),
        "functions exported by {DTS_RUNNER_INTERFACE} but not listed in the golden: {extra_in_dts:?}"
    );

    let golden_set: BTreeSet<&str> = golden.iter().map(String::as_str).collect();
    let dts_set: BTreeSet<&str> = dts.iter().map(String::as_str).collect();
    assert_eq!(
        golden_set, dts_set,
        "signature sets differ: golden documents signatures the d.ts no longer emits, \
         or the d.ts emits signatures the golden does not document"
    );

    assert_eq!(
        golden.len(),
        EXPECTED_RUNNER_FUNCTION_COUNT,
        "golden must document exactly {EXPECTED_RUNNER_FUNCTION_COUNT} runner functions"
    );
    assert_eq!(
        dts.len(),
        EXPECTED_RUNNER_FUNCTION_COUNT,
        "runner d.ts must export exactly {EXPECTED_RUNNER_FUNCTION_COUNT} functions"
    );
}

#[test]
fn browser_runner_namespace_reexported_in_sdk_dts() {
    let root = repo_root();
    let sdk_dts = read_repo_file(&root, DTS_SDK_ROOT);

    assert!(
        sdk_dts.contains("export * as runner from './interfaces/antikythera-agent-sdk-runner.js';"),
        "{DTS_SDK_ROOT} must re-export the `runner` namespace from the runner interface"
    );
}

#[test]
/// Shared payload contract golden — artifact integrity NOTE ON SCOPE: the payload *semantics*
/// (key names of the wire JSON) live in Rust structs and are verified by inspection of
/// `wasm_agent` (see contracts/shared/README.md). This test only mechanically guards the golden
/// artifact itself: valid JSON, exact top-level entries, non-empty type/fields.
///
fn payload_contract_shapes_match_golden() {
    let root = repo_root();
    let raw = read_repo_file(&root, GOLDEN_PAYLOAD_CONTRACT);
    let json: serde_json::Value =
        serde_json::from_str(&raw).expect("payload contract golden must be valid JSON");

    let top = json
        .as_object()
        .expect("payload contract golden must be a JSON object");

    let expected_top: BTreeSet<&str> = [
        "prepared_turn",
        "commit_result",
        "tool_result",
        "tool_result_inner",
    ]
    .into_iter()
    .collect();
    let actual_top: BTreeSet<&str> = top.keys().map(String::as_str).collect();
    assert_eq!(
        actual_top, expected_top,
        "payload contract golden top-level entries drifted"
    );

    for (name, entry) in top {
        let obj = entry
            .as_object()
            .unwrap_or_else(|| panic!("payload contract entry `{name}` must be a JSON object"));
        let type_name = obj.get("type").and_then(|v| v.as_str()).unwrap_or_default();
        assert!(
            !type_name.is_empty(),
            "payload contract entry `{name}` must declare a non-empty `type`"
        );
        let fields = obj
            .get("fields")
            .and_then(|v| v.as_object())
            .unwrap_or_else(|| {
                panic!("payload contract entry `{name}` must declare a `fields` object")
            });
        assert!(
            !fields.is_empty(),
            "payload contract entry `{name}` must declare at least one field"
        );
    }
}

#[test]
/// Composite-component world contract (toolrunner composition) Contract clause (per
/// `wit/antikythera.wit` + the `wasm-tools compose` wiring): the composite
/// `dist/antikythera-sdk.wasm` (SDK + toolrunner) MUST expose a single world whose imports are
/// `wasi:`-prefixed plus EXACTLY ONE antikythera import —
/// `antikythera:agent-sdk/runtime-hooks@1.0.0`, the host-wired runtime bridge (NOT supplied by
/// composition) — and whose export set pins `antikythera:agent-sdk/runner@1.0.0`. The
/// `tool-registry` and `logic-hooks` interfaces are consumed internally by the composite and MUST
/// NOT leak as imports. Falsification: `wasm-tools component wit` renders the composite's world
/// as text; any `import antikythera:` line other than the single runtime-hooks import, any
/// non-wasi import besides it, or a missing runner export goes RED. Assertions are substring
/// searches — never line-order dependent — so the test stays deterministic across tool versions.
/// Artifacts and wasm-tools are required; CI runs this suite after component builds.
///
/// (a) EXACTLY ONE antikythera import leaks out of the composite, and it must be the host-wired
/// `runtime-hooks`. `tool-registry` and `logic-hooks` are consumed internally by composition and
/// MUST NOT leak as imports.
///
/// (b) the runner interface is exported with the pinned package id.
///
/// (c) every remaining import is wasi-only, EXCEPT the single runtime-hooks.
///
/// (d) the runtime-hooks import carries the three pinned decision functions.
///
fn composed_component_world_single_runtime_hooks_import() {
    let root = repo_root();

    let wasm_path = root.join(COMPOSED_WASM_REL);
    assert!(
        wasm_path.is_file(),
        "Required artifact missing: {COMPOSED_WASM_REL}; build components before acceptance tests"
    );

    let wasm_tools =
        find_wasm_tools().expect("wasm-tools is required for Component Model acceptance tests");

    let output = std::process::Command::new(&wasm_tools)
        .args(["component", "wit"])
        .arg(&wasm_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to execute {wasm_tools:?}: {e}"));

    assert!(
        output.status.success(),
        "wasm-tools component wit failed (status: {}):\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let wit_text = String::from_utf8_lossy(&output.stdout);

    let antikythera_imports: Vec<&str> = wit_text
        .lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("import antikythera:"))
        .collect();
    assert_eq!(
        antikythera_imports.len(),
        1,
        "composite world must import exactly one antikythera interface \
         (runtime-hooks); found: {antikythera_imports:?} — tool-registry and \
         logic-hooks must be consumed internally by the composite, never imported"
    );
    assert!(
        antikythera_imports[0].starts_with(COMPOSED_EXPECTED_RUNTIME_HOOKS_IMPORT),
        "the composite's single antikythera import must be \
         `{COMPOSED_EXPECTED_RUNTIME_HOOKS_IMPORT}`; found: {}",
        antikythera_imports[0]
    );

    assert!(
        wit_text.contains(COMPOSED_EXPECTED_RUNNER_EXPORT),
        "composite world must export `{COMPOSED_EXPECTED_RUNNER_EXPORT}`"
    );

    let non_wasi_imports: Vec<&str> = wit_text
        .lines()
        .map(str::trim_start)
        .filter(|line| {
            line.starts_with("import ")
                && !line.starts_with(COMPOSED_IMPORT_PREFIX)
                && !line.starts_with(COMPOSED_EXPECTED_RUNTIME_HOOKS_IMPORT)
        })
        .collect();
    assert!(
        non_wasi_imports.is_empty(),
        "composite world imports a non-wasi interface besides runtime-hooks: {non_wasi_imports:?}"
    );

    for signature in EXPECTED_RUNTIME_HOOK_SIGNATURES {
        assert!(
            wit_text.contains(signature),
            "composite world must declare the runtime-hooks function `{signature}`"
        );
    }
}

/// Body (text between the braces) of `interface {interface_name}` in the WIT.
/// Logic-hooks contract — WIT ↔ golden ↔ generated bindings (two-way) Contract clause (per
/// `wit/antikythera.wit`): interface `logic-hooks` declares exactly three functions with pinned
/// signatures (`prepare-turn`, `decide-action`, `handle-tool-result`); world
/// `antikythera-agent-sdk` imports `logic-hooks` alongside `tool-registry` and exports `runner`;
/// world `logic-hooks-component` exports `logic-hooks`. The golden
/// `contracts/shared/wit_signatures.golden.txt` must document these signatures and the SDK's
/// generated bindings (`antikythera-sdk/src/bindings.rs`) must expose them as Rust functions.
/// Falsification: renaming/retyping/removing a logic-hooks function in the WIT without
/// regenerating the golden, or a wit-bindgen rerun that drops a binding, goes RED. The bindings
/// leg is SKIP-safe: `bindings.rs` is generated and gitignored; when it is absent (fresh checkout
/// without a build) the test still verifies the WIT↔golden legs and only skips the bindings leg.
///
fn extract_wit_interface_body(wit: &str, interface_name: &str) -> String {
    let marker = format!("interface {interface_name} {{");
    let start = wit
        .find(&marker)
        .unwrap_or_else(|| panic!("{WIT_ANTIKYTHERA} must declare `interface {interface_name}`"))
        + marker.len();
    let mut depth = 1usize;
    let bytes = wit[start..].as_bytes();
    let mut end = wit.len();
    for (i, b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = start + i;
                    break;
                }
            }
            _ => {}
        }
    }
    assert_eq!(
        depth, 0,
        "interface `{interface_name}` must be brace-balanced in {WIT_ANTIKYTHERA}"
    );
    wit[start..end].to_string()
}

/// Body (text between the braces) of `world {world_name}` in the WIT.
fn extract_wit_world_body(wit: &str, world_name: &str) -> String {
    let marker = format!("world {world_name} {{");
    let start = wit
        .find(&marker)
        .unwrap_or_else(|| panic!("{WIT_ANTIKYTHERA} must declare `world {world_name}`"))
        + marker.len();
    let mut depth = 1usize;
    let bytes = wit[start..].as_bytes();
    let mut end = wit.len();
    for (i, b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = start + i;
                    break;
                }
            }
            _ => {}
        }
    }
    assert_eq!(
        depth, 0,
        "world `{world_name}` must be brace-balanced in {WIT_ANTIKYTHERA}"
    );
    wit[start..end].to_string()
}

/// Bare signatures (`name: func(...) -> ret`) of every function declared in an
/// interface body, in WIT definition order.
fn parse_wit_func_signatures(body: &str) -> Vec<String> {
    body.lines()
        .map(str::trim)
        .filter_map(|line| {
            let line = line.strip_suffix(';').unwrap_or(line).trim();
            let colon = line.find(": func(")?;
            let name = line[..colon].trim();
            let rest = line[colon + 1..].trim();
            Some(format!("{name}: {rest}"))
        })
        .collect()
}

#[test]
/// Leg 1 — the WIT declares exactly the three pinned functions.
///
/// Leg 2 — the golden documents each signature as an individual line.
///
/// Leg 3 — the golden documents the interface block with all three hooks.
///
/// Leg 4 — the golden documents both world lines affected by logic-hooks.
///
/// Leg 5 — the generated SDK bindings expose the three hooks as Rust functions (two-way
/// WIT→bindings mapping; kebab-case becomes snake_case). SKIP-safe: `bindings.rs` is generated
/// and gitignored; absent on a fresh checkout without a build.
///
fn logic_hooks_signatures_match_wit() {
    let root = repo_root();
    let wit = read_repo_file(&root, WIT_ANTIKYTHERA);
    let golden = read_repo_file(&root, GOLDEN_WIT_SIGS);

    let body = extract_wit_interface_body(&wit, "logic-hooks");
    let wit_signatures = parse_wit_func_signatures(&body);
    let expected: BTreeSet<&str> = EXPECTED_LOGIC_HOOK_SIGNATURES.into_iter().collect();
    let actual: BTreeSet<&str> = wit_signatures.iter().map(String::as_str).collect();
    assert_eq!(
        actual, expected,
        "interface `logic-hooks` in {WIT_ANTIKYTHERA} must declare exactly \
         prepare-turn, decide-action, handle-tool-result with the pinned signatures"
    );

    let golden_lines: BTreeSet<String> = parse_golden_signatures(&golden).into_iter().collect();
    for signature in EXPECTED_LOGIC_HOOK_SIGNATURES {
        assert!(
            golden_lines.contains(signature),
            "{GOLDEN_WIT_SIGS} must document `{signature}` as a line"
        );
    }

    assert!(
        golden.contains(GOLDEN_LOGIC_HOOKS_BLOCK),
        "{GOLDEN_WIT_SIGS} must document the `logic-hooks {{ ... }}` interface block"
    );
    for signature in EXPECTED_LOGIC_HOOK_SIGNATURES {
        assert!(
            golden.contains(signature),
            "{GOLDEN_WIT_SIGS} logic-hooks block must contain `{signature}`"
        );
    }

    assert!(
        golden.contains(GOLDEN_LOGIC_HOOKS_COMPONENT_WORLD),
        "{GOLDEN_WIT_SIGS} must document `{GOLDEN_LOGIC_HOOKS_COMPONENT_WORLD}`"
    );
    assert!(
        golden.contains(GOLDEN_SDK_WORLD_WITH_HOOKS),
        "{GOLDEN_WIT_SIGS} must document `{GOLDEN_SDK_WORLD_WITH_HOOKS}` \
         (the SDK world imports logic-hooks)"
    );

    let bindings_path = root.join(SDK_BINDINGS_REL);
    if !bindings_path.is_file() {
        eprintln!(
            "Skipping bindings leg: {SDK_BINDINGS_REL} not found \
             (generated, gitignored; fresh checkout without a build)"
        );
    } else {
        let bindings = fs::read_to_string(&bindings_path)
            .unwrap_or_else(|e| panic!("cannot read generated bindings {SDK_BINDINGS_REL}: {e}"));
        assert!(
            bindings.contains("pub mod logic_hooks"),
            "{SDK_BINDINGS_REL} must declare `pub mod logic_hooks`"
        );
        for function in EXPECTED_LOGIC_HOOK_FUNCTIONS {
            let needle = format!("pub fn {function}");
            assert!(
                bindings.contains(&needle),
                "{SDK_BINDINGS_REL} must declare `{needle}` (two-way mapping \
                 from WIT function to generated Rust binding)"
            );
        }
    }
}

#[test]
/// Runtime-hooks contract — WIT ↔ golden ↔ generated bindings (two-way) Contract clause (per
/// `wit/antikythera.wit`): interface `runtime-hooks` declares exactly three functions with pinned
/// signatures (`prepare-turn`, `decide-action`, `handle-tool-result`) — the runtime-wired twin of
/// `logic-hooks`, with IDENTICAL signatures and JSON-object semantics but lower precedence (A1a:
/// the composed `logic-hooks` provider is consulted first; `runtime-hooks` runs only on
/// passthrough). World `antikythera-agent-sdk` imports `runtime-hooks` alongside `tool-registry`
/// and `logic-hooks` and exports `runner`. The golden
/// `contracts/shared/wit_signatures.golden.txt` must document the interface block and the SDK
/// world line; and the generated bindings (`antikythera-sdk/src/bindings.rs`) must expose the
/// three functions as Rust functions. Falsification: renaming/retyping/removing a runtime-hooks
/// function in the WIT without regenerating the golden, or a wit-bindgen rerun that drops a
/// binding, goes RED. The bindings leg is SKIP-safe: `bindings.rs` is generated and gitignored;
/// when it is absent (fresh checkout without a build) the test still verifies the WIT↔golden legs
/// and only skips the bindings leg.
///
/// Leg 1 — the WIT declares exactly the three pinned functions.
///
/// Leg 2 — the golden documents each signature as an individual line (shared with the identical
/// logic-hooks signatures).
///
/// Leg 3 — the golden documents the interface block with all three hooks.
///
/// Leg 4 — the golden documents the SDK world line importing runtime-hooks.
///
/// Leg 5 — the generated SDK bindings expose the three hooks as Rust functions (two-way
/// WIT→bindings mapping; kebab-case becomes snake_case). SKIP-safe: `bindings.rs` is generated
/// and gitignored; absent on a fresh checkout without a build.
///
fn runtime_hooks_signatures_match_wit() {
    let root = repo_root();
    let wit = read_repo_file(&root, WIT_ANTIKYTHERA);
    let golden = read_repo_file(&root, GOLDEN_WIT_SIGS);

    let body = extract_wit_interface_body(&wit, "runtime-hooks");
    let wit_signatures = parse_wit_func_signatures(&body);
    let expected: BTreeSet<&str> = EXPECTED_RUNTIME_HOOK_SIGNATURES.into_iter().collect();
    let actual: BTreeSet<&str> = wit_signatures.iter().map(String::as_str).collect();
    assert_eq!(
        actual, expected,
        "interface `runtime-hooks` in {WIT_ANTIKYTHERA} must declare exactly \
         prepare-turn, decide-action, handle-tool-result with the pinned signatures"
    );

    let golden_lines: BTreeSet<String> = parse_golden_signatures(&golden).into_iter().collect();
    for signature in EXPECTED_RUNTIME_HOOK_SIGNATURES {
        assert!(
            golden_lines.contains(signature),
            "{GOLDEN_WIT_SIGS} must document `{signature}` as a line"
        );
    }

    assert!(
        golden.contains(GOLDEN_RUNTIME_HOOKS_BLOCK),
        "{GOLDEN_WIT_SIGS} must document the `runtime-hooks {{ ... }}` interface block"
    );
    for signature in EXPECTED_RUNTIME_HOOK_SIGNATURES {
        assert!(
            golden.contains(signature),
            "{GOLDEN_WIT_SIGS} runtime-hooks block must contain `{signature}`"
        );
    }

    assert!(
        golden.contains(GOLDEN_SDK_WORLD_WITH_HOOKS),
        "{GOLDEN_WIT_SIGS} must document `{GOLDEN_SDK_WORLD_WITH_HOOKS}` \
         (the SDK world imports runtime-hooks)"
    );

    let bindings_path = root.join(SDK_BINDINGS_REL);
    if !bindings_path.is_file() {
        eprintln!(
            "Skipping bindings leg: {SDK_BINDINGS_REL} not found \
             (generated, gitignored; fresh checkout without a build)"
        );
    } else {
        let bindings = fs::read_to_string(&bindings_path)
            .unwrap_or_else(|e| panic!("cannot read generated bindings {SDK_BINDINGS_REL}: {e}"));
        assert!(
            bindings.contains("pub mod runtime_hooks"),
            "{SDK_BINDINGS_REL} must declare `pub mod runtime_hooks`"
        );
        for function in EXPECTED_RUNTIME_HOOK_FUNCTIONS {
            let needle = format!("pub fn {function}");
            assert!(
                bindings.contains(&needle),
                "{SDK_BINDINGS_REL} must declare `{needle}` (two-way mapping \
                 from WIT function to generated Rust binding)"
            );
        }
    }
}

#[test]
/// Standalone SDK component contract (uncomposed build artifact) Contract clause (per
/// `wit/antikythera.wit`, world `antikythera-agent-sdk`): the standalone SDK component imports
/// `tool-registry`, `logic-hooks`, AND `runtime-hooks` (the first two are supplied by
/// composition; `runtime-hooks` is wired by the host at runtime) and exports `runner`. This is
/// the pre-composition artifact; the composite (`dist/antikythera-sdk.wasm`) is covered by
/// `composed_component_world_single_runtime_hooks_import`. Artifacts and wasm-tools are required;
/// CI runs this suite after component builds.
///
fn standalone_sdk_imports_hooks_and_tools() {
    let root = repo_root();
    let wasm_path = root.join(STANDALONE_SDK_WASM_REL);
    assert!(
        wasm_path.is_file(),
        "Required artifact missing: {STANDALONE_SDK_WASM_REL}; build components before acceptance tests"
    );

    let wasm_tools =
        find_wasm_tools().expect("wasm-tools is required for Component Model acceptance tests");

    let output = std::process::Command::new(&wasm_tools)
        .args(["component", "wit"])
        .arg(&wasm_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to execute {wasm_tools:?}: {e}"));

    assert!(
        output.status.success(),
        "wasm-tools component wit failed on the standalone SDK (status: {}):\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let wit_text = String::from_utf8_lossy(&output.stdout);

    assert!(
        wit_text.contains(STANDALONE_IMPORT_TOOL_REGISTRY),
        "standalone SDK world must import {STANDALONE_IMPORT_TOOL_REGISTRY}"
    );
    assert!(
        wit_text.contains(STANDALONE_IMPORT_LOGIC_HOOKS),
        "standalone SDK world must import {STANDALONE_IMPORT_LOGIC_HOOKS}"
    );
    assert!(
        wit_text.contains(STANDALONE_IMPORT_RUNTIME_HOOKS),
        "standalone SDK world must import {STANDALONE_IMPORT_RUNTIME_HOOKS}"
    );
    assert!(
        wit_text.contains(STANDALONE_EXPORT_RUNNER),
        "standalone SDK world must export {STANDALONE_EXPORT_RUNNER}"
    );
    for signature in EXPECTED_LOGIC_HOOK_SIGNATURES {
        assert!(
            wit_text.contains(signature),
            "standalone SDK world must declare the logic-hooks function `{signature}`"
        );
    }
}

#[test]
/// Logic-core drop-in contract — world `logic-core-component` Contract clause (per
/// `wit/antikythera.wit`): world `logic-core-component` declares the contract for a custom
/// `runner` implementation that can replace the composite SDK component without any change to
/// host code. It imports `host-imports` and `tool-registry` (both OPTIONAL — unused imports are
/// legal WIT and are pruned by the component encoder) and exports `runner`. The golden
/// `contracts/shared/wit_signatures.golden.txt` must document the world line verbatim, so WIT ↔
/// golden agreement is checked in both directions: the WIT must declare the world with exactly
/// these members, and the golden must carry the canonical world line. Falsification:
/// renaming/removing a world member in the WIT, or dropping/ rewriting the golden line, goes RED.
///
/// Leg 1 — the WIT declares the world and its three members verbatim.
///
/// Leg 2 — the golden documents the canonical world line verbatim (WIT → golden direction).
///
/// Leg 3 — the golden line reconstructs exactly the members declared in the WIT (golden → WIT
/// direction): no golden-only member invented.
///
fn logic_core_world_documented() {
    let root = repo_root();
    let wit = read_repo_file(&root, WIT_ANTIKYTHERA);
    let golden = read_repo_file(&root, GOLDEN_WIT_SIGS);

    let body = extract_wit_world_body(&wit, "logic-core-component");
    let declarations: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    let expected: BTreeSet<&str> = WIT_LOGIC_CORE_MEMBERS.into_iter().collect();
    let actual: BTreeSet<&str> = declarations.iter().map(String::as_str).collect();
    assert_eq!(
        actual, expected,
        "world `logic-core-component` in {WIT_ANTIKYTHERA} must declare exactly \
         `import host-imports;`, `import tool-registry;`, `export runner;`"
    );

    assert!(
        golden.contains(GOLDEN_LOGIC_CORE_COMPONENT_WORLD),
        "{GOLDEN_WIT_SIGS} must document `{GOLDEN_LOGIC_CORE_COMPONENT_WORLD}`"
    );

    assert!(
        golden.contains(WIT_LOGIC_CORE_WORLD_MARKER),
        "{GOLDEN_WIT_SIGS} must contain the world marker `{WIT_LOGIC_CORE_WORLD_MARKER}`"
    );
    for member in WIT_LOGIC_CORE_MEMBERS {
        assert!(
            golden.contains(member),
            "{GOLDEN_WIT_SIGS} logic-core world line must contain `{member}`"
        );
    }
}

#[test]
/// Logic-core host example artifact — host-imports activation proof Contract clause (per
/// `examples/logic-core-host-example`, world `logic-core-component`): the host-llm-agent logic
/// core runs its FULL custom loop through the `host-imports` escape hatch (`call-llm`,
/// `save-state` / `load-state`, `emit-tool-call`, `log-message`). Because the custom hooks
/// actually REFERENCE the `host_*` helpers, the `host-imports` import must SURVIVE
/// component-encoder pruning — this is the activation proof that the escape hatch is wired, not
/// merely declared. The vocabulary interface is a transitive dependency of `host-imports` (its
/// records are consumed via `use vocabulary.{...}`), so it must be imported too. The exported
/// `runner` must still be the identical 16-function swap-able surface. The optional
/// `tool-registry` import is NOT referenced by this example and must be pruned. Artifacts and
/// wasm-tools are required; CI runs this suite after component builds.
///
/// (a) activation proof — the host-imports import SURVIVED encoder pruning because the custom
/// loop actually references it.
///
/// (b) the vocabulary interface is imported as the transitive dependency of host-imports (its
/// records back the host-imports signatures).
///
/// (c) the runner interface is exported with the pinned package id.
///
/// (d) every one of the 16 SDK runner functions is present (identical swap-able surface), each as
/// a kebab-case declaration.
///
/// (e) the optional `tool-registry` import is NOT referenced by this example and must be pruned
/// by the component encoder.
///
fn logic_core_host_example_imports_host_imports() {
    let root = repo_root();
    let wasm_path = root.join(LOGIC_CORE_HOST_EXAMPLE_WASM_REL);
    assert!(
        wasm_path.is_file(),
        "Required artifact missing: {LOGIC_CORE_HOST_EXAMPLE_WASM_REL}; build components before acceptance tests"
    );

    let wasm_tools =
        find_wasm_tools().expect("wasm-tools is required for Component Model acceptance tests");

    let output = std::process::Command::new(&wasm_tools)
        .args(["component", "wit"])
        .arg(&wasm_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to execute {wasm_tools:?}: {e}"));

    assert!(
        output.status.success(),
        "wasm-tools component wit failed on the logic-core host example (status: {}):\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let wit_text = String::from_utf8_lossy(&output.stdout);

    assert!(
        wit_text.contains(HOST_IMPORTS_IMPORT),
        "logic-core host example world must import `{HOST_IMPORTS_IMPORT}`; \
         the custom loop references host-imports, so the import must survive \
         component pruning (activation proof)"
    );

    assert!(
        wit_text.contains(VOCABULARY_IMPORT),
        "logic-core host example world must import `{VOCABULARY_IMPORT}` \
         (host-imports consumes the vocabulary records via `use`)"
    );

    assert!(
        wit_text.contains(LOGIC_CORE_RUNNER_EXPORT),
        "logic-core host example world must export `{LOGIC_CORE_RUNNER_EXPORT}`"
    );

    let func_decls: Vec<&str> = wit_text
        .lines()
        .map(str::trim)
        .filter(|line| line.contains(": func("))
        .collect();
    for name in LOGIC_CORE_RUNNER_FUNCTIONS {
        let needle = format!("{name}: func(");
        assert!(
            wit_text.contains(&needle),
            "logic-core host example runner must declare `{needle}`; \
             rendered runner interface: {}",
            func_decls.join("\n")
        );
    }

    assert!(
        !wit_text.contains(STANDALONE_IMPORT_TOOL_REGISTRY),
        "logic-core host example must NOT import `{STANDALONE_IMPORT_TOOL_REGISTRY}`; \
         this core never touches the toolrunner, so the optional import must be pruned"
    );
}

#[test]
/// Logic-core example artifact — identical runner export, pruned imports Contract clause (per
/// `examples/logic-core-example`, world `logic-core-component`): a drop-in logic core must export
/// the runner interface with the SAME pinned package id (`antikythera:agent-sdk/runner@1.0.0`)
/// and the SAME 16 kebab-case functions as the SDK — that is what makes the component swappable
/// without host changes. Unused optional imports (`host-imports`, `tool-registry`) must be
/// pruned: the rendered world must NOT import anything from `antikythera:*`. Artifacts and
/// wasm-tools are required; CI runs this suite after component builds.
///
/// (a) the runner interface is exported with the pinned package id.
///
/// (b) every one of the 16 SDK runner functions is present (identical swap-able surface), each as
/// a kebab-case declaration.
///
/// (c) NO import from `antikythera:*` leaks out of the example: the optional `host-imports` and
/// `tool-registry` imports are unused and must be pruned by the component encoder.
///
fn logic_core_example_exports_identical_runner() {
    let root = repo_root();
    let wasm_path = root.join(LOGIC_CORE_EXAMPLE_WASM_REL);
    assert!(
        wasm_path.is_file(),
        "Required artifact missing: {LOGIC_CORE_EXAMPLE_WASM_REL}; build components before acceptance tests"
    );

    let wasm_tools =
        find_wasm_tools().expect("wasm-tools is required for Component Model acceptance tests");

    let output = std::process::Command::new(&wasm_tools)
        .args(["component", "wit"])
        .arg(&wasm_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to execute {wasm_tools:?}: {e}"));

    assert!(
        output.status.success(),
        "wasm-tools component wit failed on the logic-core example (status: {}):\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let wit_text = String::from_utf8_lossy(&output.stdout);

    assert!(
        wit_text.contains(LOGIC_CORE_RUNNER_EXPORT),
        "logic-core example world must export `{LOGIC_CORE_RUNNER_EXPORT}`"
    );

    let func_decls: Vec<&str> = wit_text
        .lines()
        .map(str::trim)
        .filter(|line| line.contains(": func("))
        .collect();
    for name in LOGIC_CORE_RUNNER_FUNCTIONS {
        let needle = format!("{name}: func(");
        assert!(
            wit_text.contains(&needle),
            "logic-core example runner must declare `{needle}`; \
             rendered runner interface: {}",
            func_decls.join("\n")
        );
    }

    let leaked_import = wit_text
        .lines()
        .find(|line| line.trim_start().starts_with("import antikythera:"));
    assert!(
        leaked_import.is_none(),
        "logic-core example must not import any `antikythera:*` interface; \
         unused optional imports must be pruned, found: {:?}",
        leaked_import
    );
}
