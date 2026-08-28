"""Falsification tests for U04-T: runtime-hooks wiring (WasmRuntime + host.py).

Contract map (every test traces to one clause):
  C1 (audit F7): WasmRuntime must instantiate the composite
      ``dist/antikythera-sdk.wasm`` — the composite imports
      ``antikythera:agent-sdk/runtime-hooks@1.0.0`` (unwired by the current
      ``runtime.py``, which only calls ``linker.add_wasip2()``) — and
      ``call("init", ...)`` must return the configured session id without an
      unresolved-import error.
  C2a (host defaults): ``antikythera_agent.host`` exposes the three hook
      functions ``prepare_turn`` / ``decide_action`` / ``handle_tool_result``;
      with no provider configured each returns the passthrough signal
      ``{"passthrough": true}`` as a JSON string.
  C2b (host provider): a registered provider overrides the default decision;
      a provider exposing a subset leaves the unconfigured hooks on
      passthrough; clearing the provider restores passthrough.
  C3 (WIT signature): each hook takes two string arguments and returns a JSON
      string that parses as a JSON object (the WIT contract requires the
      returned string to BE a JSON object; non-object/unparseable is a hook
      failure). The provider's methods receive the WIT-fixed argument order.

Provider API pinned by this suite (TDD contract, mirroring the jco
``runtime-hooks.js`` stub — ``npm/antikythera-sdk/component/runtime-hooks.js``):
``host.set_provider(provider)`` registers an object exposing any subset of
``prepare_turn`` / ``decide_action`` / ``handle_tool_result`` methods, each
``(str, str) -> str``; ``host.get_provider()`` returns the current provider
(or ``None``); ``host.set_provider(None)`` clears. Absence of a provider (or
of a method on the provider) is never a failure — the hook returns passthrough.

Envelope (test-environment assumptions):
  - ``wasmtime>=42.0`` installed (optional dep ``[wasm]``); skipped loudly
    with an explicit reason when absent — never silently.
  - Composite artifact present at ``dist/antikythera-sdk.wasm`` (built by
    ``task build`` / compose); skipped loudly when absent.
  - ``WasmRuntime`` has no public constructor path parameter today, so the
    smoke test pins the composite through the module-level ``_WASM_PATH``
    seam (monkeypatch). The packaged default ``python/antikythera_agent/
    antikythera.wasm`` is byte-identical to the audited composite (verified
    during U04-T: SHA-256 97FCD735C038109…), so the seam is representative
    of the production default path.
"""

from __future__ import annotations

import contextlib
import json
from pathlib import Path
from typing import Iterator

import pytest

# ── Test-environment bootstrap ────────────────────────────────────────────────
# The uninstalled package is importable by putting ``python/`` on sys.path.
_PYTHON_SRC = Path(__file__).resolve().parents[1]
_REPO_ROOT = Path(__file__).resolve().parents[3]

import sys  # noqa: E402

if str(_PYTHON_SRC) not in sys.path:
    sys.path.insert(0, str(_PYTHON_SRC))

# The composite under test: the audited F7 artifact.
_COMPOSITE_WASM = _REPO_ROOT / "dist" / "antikythera-sdk.wasm"

_RUNTIME_HOOKS_IMPORT = "antikythera:agent-sdk/runtime-hooks@1.0.0"

_PASSTHROUGH = {"passthrough": True}


@pytest.fixture(scope="module")
def wasmtime():
    """wasmtime module, or a loud skip when the optional dependency is absent."""
    pytest.importorskip(
        "wasmtime",
        reason="wasmtime not installed; install with: pip install antikythera-agent[wasm] "
        "(declared optional dependency wasmtime>=42.0)",
    )
    pytest.importorskip("wasmtime.component")
    import wasmtime  # noqa: F401
    import wasmtime.component  # noqa: F401
    return wasmtime


@pytest.fixture(scope="module")
def composite_path() -> Path:
    """Composite artifact path; skips loudly when the build artifact is absent."""
    if not _COMPOSITE_WASM.exists():
        pytest.skip(
            f"composite artifact not found at {_COMPOSITE_WASM}; build it with "
            "`task build` (wasm-tools compose SDK + toolrunner + default-hooks)"
        )
    return _COMPOSITE_WASM


def _composite_imports_runtime_hooks(wasmtime, wasm_path: Path) -> bool:
    """True if the component at ``wasm_path`` imports the runtime-hooks interface."""
    engine = wasmtime.Engine()
    component = wasmtime.component.Component.from_file(engine, str(wasm_path))
    import_names = [str(name) for name in component.type.imports(engine)]
    return _RUNTIME_HOOKS_IMPORT in import_names


# ── C1 (F7): WasmRuntime composite instantiation ─────────────────────────────

def test_c1_envelope_composite_imports_runtime_hooks(wasmtime, composite_path):
    """Envelope: the artifact under test is the F7 target.

    Guards against a stale artifact producing a false GREEN — if the composite
    lacked the runtime-hooks import, the current runtime.py would instantiate
    it successfully and the smoke test would pass without proving anything.
    """
    assert _composite_imports_runtime_hooks(wasmtime, composite_path), (
        f"{composite_path} does not import {_RUNTIME_HOOKS_IMPORT}; "
        "this is not the audited composite (wasm-tools component wit shows "
        "the import on dist/antikythera-sdk.wasm)"
    )


def test_c1_wasm_runtime_instantiates_composite_and_init_returns_session_id(
    wasmtime, composite_path, monkeypatch
):
    """C1: WasmRuntime instantiates the composite and init returns the session id.

    Expectation (GREEN, after Coder wires the runtime-hooks import): calling
    ``call("init", '{"session_id": "t4-smoke"}')`` returns the bare session id
    ``"t4-smoke"`` (runner contract — antikythera-sdk runner/mod.rs returns
    the configured ``session_id`` verbatim).

    Current state (RED): ``runtime.py`` wires only ``linker.add_wasip2()``, so
    instantiation raises a wasmtime unresolved-import error for
    ``antikythera:agent-sdk/runtime-hooks@1.0.0`` (reproduced in U04-T probe).
    """
    from antikythera_agent import runtime

    monkeypatch.setattr(runtime, "_WASM_PATH", composite_path)
    rt = runtime.WasmRuntime()
    result = rt.call("init", '{"session_id": "t4-smoke"}')
    assert result == "t4-smoke"


# ── C2a/C3: host.py hook surface, defaults, and WIT signature ────────────────

def test_c2a_host_module_exposes_three_hook_functions():
    """C2a: antikythera_agent.host exposes the three runtime-hook functions."""
    from antikythera_agent import host

    for name in ("prepare_turn", "decide_action", "handle_tool_result"):
        assert callable(getattr(host, name)), f"host.{name} is not callable"


def test_c2a_default_returns_passthrough_for_all_hooks():
    """C2a: with no provider configured, every hook returns the passthrough signal.

    The passthrough signal is exactly the single-key object ``{"passthrough": true}``
    (WIT runtime-hooks doc) delivered as a JSON string.
    """
    from antikythera_agent import host

    for fn in (host.prepare_turn, host.decide_action, host.handle_tool_result):
        result = fn('{"request": 1}', '{"state": {}}')
        assert isinstance(result, str)
        assert json.loads(result) == _PASSTHROUGH


def test_c3_hooks_accept_two_strings_and_return_parseable_json_object():
    """C3: each hook takes (string, string) and returns a parseable JSON object.

    WIT contract: "The returned string MUST be a JSON object; an unparseable
    or non-object return is treated as a hook failure (Err-path semantics)."
    """
    from antikythera_agent import host

    for fn in (host.prepare_turn, host.decide_action, host.handle_tool_result):
        result = fn("request-payload", "session-state-payload")
        assert isinstance(result, str)
        parsed = json.loads(result)
        assert isinstance(parsed, dict), (
            f"{fn.__name__} returned non-object JSON: {result!r}"
        )


def test_c3_provider_receives_wit_argument_order():
    """C3: the provider's methods receive the WIT-fixed argument order.

    WIT runtime-hooks: prepare-turn(request-json, session-state-json);
    decide-action(session-state-json, llm-response-json);
    handle-tool-result(session-state-json, tool-result-json).
    """
    from antikythera_agent import host

    calls = []

    class RecordingProvider:
        def prepare_turn(self, request_json, session_state_json):
            calls.append(("prepare_turn", request_json, session_state_json))
            return '{"passthrough": true}'

        def decide_action(self, session_state_json, llm_response_json):
            calls.append(("decide_action", session_state_json, llm_response_json))
            return '{"passthrough": true}'

        def handle_tool_result(self, session_state_json, tool_result_json):
            calls.append(("handle_tool_result", session_state_json, tool_result_json))
            return '{"passthrough": true}'

    with _provider_sandbox(host):
        host.set_provider(RecordingProvider())
        host.prepare_turn("REQ", "STATE")
        host.decide_action("SESSION", "LLM")
        host.handle_tool_result("SESSION", "TOOL")

    assert calls == [
        ("prepare_turn", "REQ", "STATE"),
        ("decide_action", "SESSION", "LLM"),
        ("handle_tool_result", "SESSION", "TOOL"),
    ]


# ── C2b: provider override semantics ─────────────────────────────────────────

def test_c2b_provider_override_replaces_default_decision():
    """C2b: a registered provider's decision replaces the default passthrough."""
    from antikythera_agent import host

    class OverridingProvider:
        def decide_action(self, session_state_json, llm_response_json):
            return '{"action": "final", "content": "runtime-hook-forced-final"}'

    with _provider_sandbox(host):
        host.set_provider(OverridingProvider())
        result = host.decide_action('{"step": 1}', '{"action": "call_tool"}')

    assert json.loads(result) == {
        "action": "final",
        "content": "runtime-hook-forced-final",
    }


def test_c2b_provider_subset_leaves_other_hooks_on_passthrough():
    """C2b: a provider exposing a subset leaves the unconfigured hooks passthrough.

    Mirrors the jco stub: an absent provider method is never a failure.
    """
    from antikythera_agent import host

    class PartialProvider:
        def decide_action(self, session_state_json, llm_response_json):
            return '{"action": "final", "content": "forced"}'

    with _provider_sandbox(host):
        host.set_provider(PartialProvider())
        assert json.loads(host.prepare_turn("{}", "{}")) == _PASSTHROUGH
        assert json.loads(host.handle_tool_result("{}", "{}")) == _PASSTHROUGH
        assert json.loads(host.decide_action("{}", "{}")) == {
            "action": "final",
            "content": "forced",
        }


def test_c2b_clear_provider_restores_default_passthrough():
    """C2b: clearing the provider restores the default passthrough behavior."""
    from antikythera_agent import host

    class OverridingProvider:
        def prepare_turn(self, request_json, session_state_json):
            return '{"override": true}'

    with _provider_sandbox(host):
        host.set_provider(OverridingProvider())
        assert json.loads(host.prepare_turn("{}", "{}")) == {"override": True}
        host.set_provider(None)
        assert json.loads(host.prepare_turn("{}", "{}")) == _PASSTHROUGH


# ── State isolation for the module-level provider registry ───────────────────

@contextlib.contextmanager
def _provider_sandbox(host) -> Iterator[None]:
    """Save/restore the host provider registry so tests share no provider state."""
    provider_before = host.get_provider()
    try:
        yield
    finally:
        host.set_provider(provider_before)
