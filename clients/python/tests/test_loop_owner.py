"""Falsification suite untuk `antikythera_agent.server.loop_owner` (unit U23).

Peran Coder — verifikasi mekanis WAJIB. Suite ini memfalsifikasi kontrak
sambungan U23 (K1 tool loop host-side untuk mode core@server, mirror
`antikythera-server-runtime/src/loop_owner.rs`):

- Loop mencapai `final`; `steps` benar (1-based); `content`/`commit_json`
  berasal dari commit runner.
- Builtin in-band (`echo` builtin komposit): drain SETELAH commit memuat
  `tool_result` untuk tool tsb → TIDAK ada host execution (handler server
  counter == 0) → tidak dieksekusi dua kali (invariant U23).
- Tool server lokal: handler dieksekusi persis sekali; hasil di-feed lewat
  `process_tool_result_for_session` (tool_result muncul di drain akhir).
- Tool client remote: `tool-execution-request` SSE + POST-back memakai
  `ControlChannel` NYATA (create_correlation / is_client_connected / push /
  await_postback / resolve_postback); fail-closed tanpa client → `permission:`.
- Hook runtime (`runtime_hooks_enabled=true`): `hook-request` dikirim ke peer
  lewat `ControlChannel`; override `decide-action` dikomit sebagai final;
  denial hook → error `permission:`.
- Error path: max_steps terlampaui / LLM gagal / retry / action tak dikenal /
  tool tanpa owner → error eksplisit; denial gate → prefix `permission:`.
- Union push: `register_tools` menerima persis `registry.definitions()`.

Amplop test (asumsi yang dideklarasikan agar sertifikasi tetap sah):
- Komposit NYATA dipakai (U05): `python/antikythera_agent/antikythera.wasm`
  ATAU `dist/antikythera-sdk.wasm` (byte-identical, SHA-256 97FCD735…); seam
  `monkeypatch` `runtime._WASM_PATH` — kontrak `runtime.py` TIDAK diubah.
- `wasmtime` (opsional dep U04-T) di-`importorskip` dengan alasan eksplisit.
- Stub LLM (U14): `StubProvider` untuk satu langkah; `_ScriptedStub` untuk
  urutan framework-generic (content terakhir berulang, mirror Rust
  `ScriptedStub`).
- Routing remote/hook memakai `ControlChannel` NYATA dengan writer
  auto-resolving (resolusi POST-back sinkron di jalur push — deterministik,
  tanpa thread); test fail-closed memakai channel tanpa client.
- State `antikythera_agent.host` (registry modul-global) di-restore setelah
  tiap test.

Menjalankan (dari repo root):
    $env:PYTHONPATH="python"
    python -m pytest python/tests/test_loop_owner.py -v
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional

import pytest

# ── Test-environment bootstrap ────────────────────────────────────────────────
_PYTHON_SRC = Path(__file__).resolve().parents[1]
_REPO_ROOT = Path(__file__).resolve().parents[3]

if str(_PYTHON_SRC) not in sys.path:
    sys.path.insert(0, str(_PYTHON_SRC))

from antikythera_agent import host  # noqa: E402
from antikythera_agent import runtime as runtime_mod  # noqa: E402
from antikythera_agent.runtime import WasmRuntime  # noqa: E402
from antikythera_agent.server import provider as provider_mod  # noqa: E402
from antikythera_agent.server.control import ControlChannel  # noqa: E402
from antikythera_agent.server.gate import PolicyGate, PermissionDeniedError  # noqa: E402
from antikythera_agent.server.loop_owner import (  # noqa: E402
    LoopOutcome,
    ToolLoopConfig,
    ToolLoopError,
    run_tool_loop,
)
from antikythera_agent.server.registry import UnionRegistry  # noqa: E402

#: Komposit yang diuji: packaged default (U05) atau audited dist (identik).
_COMPOSITE_WASM = _REPO_ROOT / "dist" / "antikythera-sdk.wasm"
_PACKAGED_WASM = _PYTHON_SRC / "antikythera_agent" / "antikythera.wasm"

#: Kunci envelope golden `*_event` (WIRE_PROTOCOL §7 — nol field ekstra).
ENVELOPE_KEYS = {"type", "correlation_id", "session_id", "client_id", "payload"}


# ===========================================================================
# Fixtures
# ===========================================================================

@pytest.fixture(scope="module")
def wasmtime():
    """wasmtime module, atau skip keras bila dep opsional tidak ada."""
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
    """Path komposit NYATA; skip keras bila tidak ada (perlu `task build`)."""
    if _PACKAGED_WASM.exists():
        return _PACKAGED_WASM
    if _COMPOSITE_WASM.exists():
        return _COMPOSITE_WASM
    pytest.skip(
        "composite artifact not found (python/antikythera_agent/antikythera.wasm or "
        "dist/antikythera-sdk.wasm); build it with `task build`"
    )
    return _PACKAGED_WASM


@pytest.fixture()
def make_runtime(composite_path, monkeypatch):
    """Factory WasmRuntime NYATA; path komposit lewat seam `_WASM_PATH`."""
    def _make(runtime_cls=WasmRuntime, **kwargs):
        monkeypatch.setattr(runtime_mod, "_WASM_PATH", composite_path)
        return runtime_cls(**kwargs)

    return _make


@pytest.fixture(autouse=True)
def _restore_host_provider():
    """Registry host modul-global TIDAK boleh bocor antar test."""
    yield
    host.set_provider(None)


# ===========================================================================
# Stub / helper
# ===========================================================================

class _ScriptedStub(provider_mod.LlmProvider):
    """Provider yang mengembalikan urutan content framework-generic; content
    terakhir berulang (mirror `ScriptedStub` Rust di runtime_bridge.rs)."""

    def __init__(self, responses: List[str]) -> None:
        self._responses = list(responses)
        self._index = 0
        self.calls: List[Dict[str, Any]] = []

    def _call(self, request: Dict[str, Any]) -> Dict[str, Any]:
        index = self._index
        self._index += 1
        self.calls.append(request)
        if index < len(self._responses):
            content = self._responses[index]
        elif self._responses:
            content = self._responses[-1]
        else:
            content = ""
        return {
            "content": content,
            "model": request.get("model"),
            "session_id": request.get("session_id"),
            "message_json": None,
            "tokens_used": 4,
            "finish_reason": "stop",
            "raw_response_json": None,
        }


class _FailingProvider:
    """Provider yang selalu gagal — path LLM call failure."""

    def call(self, request: Dict[str, Any]) -> Dict[str, Any]:
        raise provider_mod.LlmError("backend down")


class _RecordingRuntime(WasmRuntime):
    """WasmRuntime nyata yang merekam hasil `drain-events` dan argumen
    `register-tools` untuk asersi in-band / union push; kontrak runtime.py
    tidak diubah (subclass + super())."""

    def __init__(self, **kwargs: Any) -> None:
        super().__init__(**kwargs)
        self.drain_results: List[Any] = []
        self.register_tools_args: List[str] = []

    def call(self, func_name: str, args: str) -> str:
        result = super().call(func_name, args)
        normalized = func_name.replace("_", "-")
        if normalized == "drain-events":
            self.drain_results.append(json.loads(result))
        elif normalized == "register-tools":
            self.register_tools_args.append(args)
        return result


class _UnusedControl:
    """Control channel yang TIDAK boleh dipakai — memastikan path tertentu
    (final-only / in-band) tidak menyentuh routing remote/hook."""

    def create_correlation(self, *args: Any, **kwargs: Any) -> str:
        raise AssertionError("control must not be used on this path")

    def is_client_connected(self, *args: Any, **kwargs: Any) -> bool:
        raise AssertionError("control must not be used on this path")

    def cancel_pending(self, *args: Any, **kwargs: Any) -> bool:
        raise AssertionError("control must not be used on this path")

    def push(self, *args: Any, **kwargs: Any) -> bool:
        raise AssertionError("control must not be used on this path")

    def await_postback(self, *args: Any, **kwargs: Any) -> Dict[str, Any]:
        raise AssertionError("control must not be used on this path")


class _AutoResolveWriter:
    """Writer SSE `ControlChannel` NYATA yang me-resolve POST-back sinkron saat
    frame `data: ...` diterima — roundtrip deterministik tanpa thread.

    Pending sudah terdaftar (create_correlation) SEBELUM push, sehingga
    resolve di jalur push aman; await_postback mengambil fast-path resolved.
    """

    def __init__(self, control: ControlChannel, postback_builder) -> None:
        self._control = control
        self._postback_builder = postback_builder
        self.frames: List[bytes] = []

    def __call__(self, data: bytes) -> None:
        self.frames.append(data)
        if not data.startswith(b"data: "):
            return
        envelope = json.loads(data[len(b"data: ") : -2])
        body = self._postback_builder(envelope)
        if body is not None:
            self._control.resolve_postback(envelope["correlation_id"], body)


def _call_then_final(tool: str, input_: Dict[str, Any]) -> List[str]:
    """Script dua langkah: call `tool` sekali, lalu final (mirror Rust)."""
    return [
        json.dumps({"action": "call_tool", "tool": tool, "input": input_}),
        json.dumps({"action": "final", "content": "after-tool"}),
    ]


def _find_tool_result(events: Any, tool: str) -> Optional[Dict[str, Any]]:
    """Event `tool_result` untuk `tool` di dalam drain, atau None."""
    if not isinstance(events, list):
        return None
    for event in events:
        if (
            isinstance(event, dict)
            and event.get("kind") == "tool_result"
            and (event.get("payload") or {}).get("tool") == tool
        ):
            return event
    return None


def _parse_frame(frame: bytes) -> Dict[str, Any]:
    """Parse frame SSE `data: {json}\\n\\n` menjadi dict."""
    assert frame.startswith(b"data: "), frame
    assert frame.endswith(b"\n\n"), frame
    return json.loads(frame[len(b"data: ") : -2])


# ===========================================================================
# 1. Konfigurasi — default mirror Rust + validasi fail-fast
# ===========================================================================

def test_config_defaults_mirror_rust_defaults():
    """ToolLoopConfig: default identik dengan `loop_owner.rs::Default` +
    `ServerRuntimeConfig` (client_id / pending_ttl 60s)."""
    config = ToolLoopConfig()
    assert config.session_id == "server-loop"
    assert config.max_steps == 10
    assert config.prompts == ["hello"]
    assert config.provider == "stub"
    assert config.model == "stub-model"
    assert config.temperature is None
    assert config.max_tokens is None
    assert config.force_json is False
    assert config.register_union_tools is True
    assert config.runtime_hooks_enabled is False
    assert config.client_id == "antikythera-client"
    assert config.pending_ttl_secs == 60.0


def test_config_rejects_invalid_values():
    """ToolLoopConfig: nilai invalid ditolak ValueError di konstruksi
    (invariant enforcement di titik masuk — mirror fail-fast unit lain)."""
    with pytest.raises(ValueError):
        ToolLoopConfig(session_id="")
    with pytest.raises(ValueError):
        ToolLoopConfig(max_steps=-1)
    with pytest.raises(ValueError):
        ToolLoopConfig(max_steps=True)
    with pytest.raises(ValueError):
        ToolLoopConfig(prompts="hello")
    with pytest.raises(ValueError):
        ToolLoopConfig(prompts=["ok", 42])
    with pytest.raises(ValueError):
        ToolLoopConfig(provider="")
    with pytest.raises(ValueError):
        ToolLoopConfig(force_json="yes")
    with pytest.raises(ValueError):
        ToolLoopConfig(client_id="")
    with pytest.raises(ValueError):
        ToolLoopConfig(pending_ttl_secs=0)
    with pytest.raises(ValueError):
        ToolLoopConfig(temperature="hot")


# ===========================================================================
# 2. Integrasi nyata — komposit WASM + loop mencapai final
# ===========================================================================

def test_final_single_step_reaches_final(make_runtime, wasmtime):
    """INTEGRASI NYATA KE FINAL: StubProvider (U14, parity Rust — content =
    SELURUH string response_json) dengan action envelope framework-generic
    `final`; loop: init → prepare → LLM → commit → final dalam satu langkah;
    `steps == 1` (1-based, mirror Rust)."""
    stub = provider_mod.StubProvider(
        '{"action": "final", "content": "hello-from-stub"}'
    )
    rt = make_runtime()
    registry = UnionRegistry()
    gate = PolicyGate()
    config = ToolLoopConfig(
        session_id="loop-final-1",
        max_steps=5,
        prompts=["sapaan"],
        provider="stub",
        model="stub-model",
    )
    outcome = run_tool_loop(rt, registry, gate, lambda name: stub, _UnusedControl(), config)

    assert isinstance(outcome, LoopOutcome)
    assert outcome.session_id == "loop-final-1"
    assert outcome.action == "final"
    assert outcome.steps == 1
    assert outcome.content == "hello-from-stub"
    assert outcome.commit_json["action"] == "final"
    assert outcome.commit_json["content"] == "hello-from-stub"


def test_default_resolver_uses_stub_provider(make_runtime, wasmtime):
    """`provider=None` memakai `resolve_provider` default: stub registry
    mengembalikan content teks-biasa `"stub response"` → commit sebagai final
    (content non-JSON = AgentAction::Final, processor.rs)."""
    rt = make_runtime()
    config = ToolLoopConfig(
        session_id="loop-default-provider", max_steps=5, prompts=["hi"]
    )
    outcome = run_tool_loop(rt, UnionRegistry(), PolicyGate(), None, _UnusedControl(), config)

    assert outcome.action == "final"
    assert outcome.content == "stub response"
    assert outcome.steps == 1


def test_llm_call_failure_wrapped(make_runtime, wasmtime):
    """LLM call gagal → ToolLoopError berprefix `tool loop: llm call failed:`."""
    rt = make_runtime()
    config = ToolLoopConfig(
        session_id="loop-llm-fail", max_steps=5, prompts=["hi"]
    )
    with pytest.raises(ToolLoopError) as excinfo:
        run_tool_loop(rt, UnionRegistry(), PolicyGate(), lambda name: _FailingProvider(), _UnusedControl(), config)
    assert "tool loop: llm call failed: backend down" in str(excinfo.value)


# ===========================================================================
# 3. Builtin in-band — tanpa host execution, tanpa double execution
# ===========================================================================

def test_builtin_in_band_no_host_execution(make_runtime, wasmtime):
    """Builtin `echo` dieksekusi komposit DI DALAM commit (tool-registry);
    drain setelah commit memuat `tool_result` → loop TIDAK routing ke host.
    Handler server yang menghitung eksekusi harus tetap 0 (bukti drain memuat
    tool_result in-band dan tidak ada double execution — mirror
    `builtin_echo_executes_in_band_without_host_execution` Rust)."""
    stub = _ScriptedStub(_call_then_final("echo", {"hi": 1}))
    host_calls: List[Any] = []

    def echo_handler(args: Any) -> Dict[str, Any]:
        host_calls.append(args)
        return {"echoed": args}

    registry = UnionRegistry()
    registry.register_server(
        {"name": "echo", "description": "reference builtin echo"}, handler=echo_handler
    )
    gate = PolicyGate()
    gate.allow_tool("server", "echo")
    config = ToolLoopConfig(
        session_id="in-band-echo", max_steps=5, prompts=["use echo"], provider="stub", model="stub"
    )
    rt = make_runtime(runtime_cls=_RecordingRuntime)
    outcome = run_tool_loop(rt, registry, gate, lambda name: stub, _UnusedControl(), config)

    assert outcome.action == "final"
    assert outcome.steps == 2
    assert outcome.content == "after-tool"
    # Drain pertama (setelah commit call_tool) memuat tool_result in-band.
    assert rt.drain_results, "loop harus men-drain events pada aksi call_tool"
    in_band = _find_tool_result(rt.drain_results[0], "echo")
    assert in_band is not None, f"drain in-band harus memuat tool_result echo: {rt.drain_results[0]}"
    assert in_band["payload"]["success"] is True
    # Bukti tanpa host execution: handler server tidak pernah dipanggil.
    assert host_calls == [], "builtin in-band TIDAK boleh dieksekusi host (double execution)"
    # Union push: register_tools menerima persis definisi union registry.
    assert rt.register_tools_args, "register_union_tools=True harus memanggil register_tools"
    assert json.loads(rt.register_tools_args[0]) == registry.definitions()


def test_register_union_tools_false_skips_push(make_runtime, wasmtime):
    """`register_union_tools=False` → register_tools TIDAK dipanggil."""
    stub = _ScriptedStub(['{"action": "final", "content": "done"}'])
    registry = UnionRegistry()
    registry.register_server({"name": "echo", "description": "echo"}, handler=lambda a: a)
    config = ToolLoopConfig(
        session_id="loop-no-union", max_steps=5, prompts=["hi"],
        register_union_tools=False,
    )
    rt = make_runtime(runtime_cls=_RecordingRuntime)
    outcome = run_tool_loop(rt, registry, PolicyGate(), lambda name: stub, _UnusedControl(), config)

    assert outcome.action == "final"
    assert rt.register_tools_args == []


# ===========================================================================
# 4. Tool server lokal — handler melalui loop → process-tool-result
# ===========================================================================

def test_server_local_tool_executes_via_handler(make_runtime, wasmtime):
    """Tool owner `server` (non-builtin): routing lokal memanggil handler
    persis sekali dengan args LLM; hasil di-feed lewat
    `process_tool_result_for_session` (tool_result sukses di drain akhir)."""
    stub = _ScriptedStub(_call_then_final("server_time", {"tz": "UTC"}))
    handler_calls: List[Any] = []

    def server_time_handler(args: Any) -> Dict[str, Any]:
        handler_calls.append(args)
        return {"datetime": "2026-08-12T00:00:00Z", "args": args}

    registry = UnionRegistry()
    registry.register_server(
        {"name": "server_time", "description": "server-only deterministic clock"},
        handler=server_time_handler,
    )
    gate = PolicyGate()
    gate.allow_tool("server", "server_time")
    config = ToolLoopConfig(
        session_id="local-tool-server", max_steps=5, prompts=["what time is it"],
        provider="stub", model="stub",
    )
    rt = make_runtime()
    outcome = run_tool_loop(rt, registry, gate, lambda name: stub, _UnusedControl(), config)

    assert outcome.action == "final"
    assert outcome.steps == 2
    assert outcome.content == "after-tool"
    assert handler_calls == [{"tz": "UTC"}], "handler lokal dipanggil persis sekali"
    drain = json.loads(rt.call("drain_events", outcome.session_id))
    tool_result = _find_tool_result(drain, "server_time")
    assert tool_result is not None, f"drain harus memuat tool_result server_time: {drain}"
    assert tool_result["payload"]["success"] is True


def test_server_local_handler_failure_is_result_not_error(make_runtime, wasmtime):
    """Kegagalan handler lokal adalah hasil `success=false`, BUKAN error loop
    (mirror `routing.rs::execute_local` Err-path)."""
    stub = _ScriptedStub(_call_then_final("boom", {}))

    def boom_handler(args: Any) -> Dict[str, Any]:
        raise RuntimeError("handler exploded")

    registry = UnionRegistry()
    registry.register_server({"name": "boom", "description": "fails"}, handler=boom_handler)
    gate = PolicyGate()
    gate.allow_tool("server", "boom")
    config = ToolLoopConfig(
        session_id="local-tool-fail", max_steps=5, prompts=["boom"], provider="stub", model="stub"
    )
    rt = make_runtime()
    outcome = run_tool_loop(rt, registry, gate, lambda name: stub, _UnusedControl(), config)

    assert outcome.action == "final"
    drain = json.loads(rt.call("drain_events", outcome.session_id))
    tool_result = _find_tool_result(drain, "boom")
    assert tool_result is not None, f"drain harus memuat tool_result boom: {drain}"
    assert tool_result["payload"]["success"] is False


# ===========================================================================
# 5. Tool client remote — ControlChannel NYATA (SSE + POST-back)
# ===========================================================================

def test_remote_client_tool_roundtrips_real_control_channel(make_runtime, wasmtime):
    """Tool owner `client`: routing remote memakai `ControlChannel` NYATA —
    `create_correlation` / `is_client_connected` / `push` /
    `await_postback`; envelope shape golden `tool_execution_request_event`;
    hasil POST-back dikonsumsi loop (tool_result sukses di drain akhir)."""
    stub = _ScriptedStub(_call_then_final("client_secret", {"ask": "secret"}))
    registry = UnionRegistry()
    registry.register_client(
        {"name": "client_secret", "description": "client-side secret tool"}
    )
    gate = PolicyGate()
    gate.allow_tool("client", "client_secret")
    control = ControlChannel(keepalive_interval=3600.0)

    def postback_builder(envelope: Dict[str, Any]):
        if envelope["type"] == "tool-execution-request":
            return {
                "correlation_id": envelope["correlation_id"],
                "ok": True,
                "payload": {
                    "tool-name": "client_secret",
                    "success": True,
                    "output-json": '{"secret": "opensecret"}',
                    "error-message": None,
                    "step-id": 0,
                },
                "error": None,
            }
        return None

    writer = _AutoResolveWriter(control, postback_builder)
    control.register_client("antikythera-client", writer)
    config = ToolLoopConfig(
        session_id="remote-tool-server", max_steps=5, prompts=["read the secret"],
        provider="stub", model="stub", client_id="antikythera-client", pending_ttl_secs=10.0,
    )
    rt = make_runtime()
    outcome = run_tool_loop(rt, registry, gate, lambda name: stub, control, config)

    assert outcome.action == "final"
    assert outcome.steps == 2
    assert outcome.content == "after-tool"
    # Envelope `tool-execution-request` dikirim ke client (shape golden).
    frames = [f for f in writer.frames if b"tool-execution-request" in f]
    assert frames, "harus ada frame tool-execution-request di SSE"
    request = _parse_frame(frames[0])
    assert set(request) == ENVELOPE_KEYS
    assert request["type"] == "tool-execution-request"
    assert request["client_id"] == "antikythera-client"
    assert request["session_id"] == "remote-tool-server"
    assert request["payload"]["tool-name"] == "client_secret"
    assert request["payload"]["arguments-json"] == '{"ask": "secret"}'
    assert request["payload"]["step-id"] == 1
    # Hasil POST-back dikonsumsi: tool_result sukses di drain akhir.
    drain = json.loads(rt.call("drain_events", outcome.session_id))
    tool_result = _find_tool_result(drain, "client_secret")
    assert tool_result is not None, f"drain harus memuat tool_result client_secret: {drain}"
    assert tool_result["payload"]["success"] is True
    assert control.pending_len() == 0, "pending POST-back harus habis dikonsumsi"


def test_remote_client_tool_denied_without_client(make_runtime, wasmtime):
    """Remote tanpa client terhubung → fail-closed `permission:` dan pending
    di-cancel (tidak ada silent hang / pending bocor)."""
    stub = _ScriptedStub(_call_then_final("client_secret", {}))
    registry = UnionRegistry()
    registry.register_client({"name": "client_secret", "description": "secret"})
    gate = PolicyGate()
    gate.allow_tool("client", "client_secret")
    control = ControlChannel(keepalive_interval=3600.0)
    config = ToolLoopConfig(
        session_id="remote-no-client", max_steps=5, prompts=["read"],
        provider="stub", model="stub", client_id="antikythera-client", pending_ttl_secs=10.0,
    )
    rt = make_runtime()
    with pytest.raises(ToolLoopError) as excinfo:
        run_tool_loop(rt, registry, gate, lambda name: stub, control, config)
    assert str(excinfo.value).startswith("permission: ")
    assert "requires a connected client" in str(excinfo.value)
    assert control.pending_len() == 0, "pending harus di-cancel (fail-closed hygiene)"


def test_remote_gate_denial_permission_prefix(make_runtime, wasmtime):
    """Tool client terdaftar tapi TIDAK di-allowlist → denial gate
    `PermissionDeniedError` berprefix `permission:` (default-deny R4)."""
    stub = _ScriptedStub(_call_then_final("client_secret", {}))
    registry = UnionRegistry()
    registry.register_client({"name": "client_secret", "description": "secret"})
    gate = PolicyGate()  # default-deny: client_secret TIDAK di-allow
    control = ControlChannel(keepalive_interval=3600.0)
    control.register_client("antikythera-client", lambda data: None)
    config = ToolLoopConfig(
        session_id="remote-denied", max_steps=5, prompts=["read"],
        provider="stub", model="stub", client_id="antikythera-client", pending_ttl_secs=10.0,
    )
    rt = make_runtime()
    with pytest.raises(PermissionDeniedError) as excinfo:
        run_tool_loop(rt, registry, gate, lambda name: stub, control, config)
    assert str(excinfo.value).startswith("permission: ")
    assert "client_secret" in str(excinfo.value)


def test_unknown_tool_denied_permission_prefix(make_runtime, wasmtime):
    """Tool tanpa owner di union registry → denial `permission:` (mirror
    `routing.rs::resolve_destination` unknown → not in allowlist)."""
    stub = _ScriptedStub(_call_then_final("never_registered", {}))
    config = ToolLoopConfig(
        session_id="unknown-tool", max_steps=5, prompts=["use it"],
        provider="stub", model="stub",
    )
    rt = make_runtime()
    with pytest.raises(ToolLoopError) as excinfo:
        run_tool_loop(rt, UnionRegistry(), PolicyGate(), lambda name: stub, _UnusedControl(), config)
    assert str(excinfo.value).startswith("permission: ")
    assert "never_registered" in str(excinfo.value)


# ===========================================================================
# 6. Hook runtime — hook-request melalui ControlChannel NYATA
# ===========================================================================

def test_hook_decision_routes_through_real_control_channel(make_runtime, wasmtime):
    """`runtime_hooks_enabled=true`: hook-request dikirim ke peer lewat
    `ControlChannel` (shape golden `hook_request_event`); override
    `decide-action` dikomit sebagai final (content client-hook-decision —
    mirror `hook_request_roundtrips_over_sse_and_override_is_committed`)."""
    stub = _ScriptedStub(['{"action": "final", "content": "server-default"}'])
    registry = UnionRegistry()
    gate = PolicyGate()
    gate.allow_hook("prepare-turn")
    gate.allow_hook("decide-action")
    control = ControlChannel(keepalive_interval=3600.0)

    def postback_builder(envelope: Dict[str, Any]):
        if envelope["type"] == "hook-request":
            hook = envelope["payload"]["hook"]
            if hook == "decide-action":
                return {
                    "correlation_id": envelope["correlation_id"],
                    "ok": True,
                    "payload": {"action": "final", "content": "client-hook-decision"},
                    "error": None,
                }
            return {
                "correlation_id": envelope["correlation_id"],
                "ok": True,
                "payload": {"passthrough": True},
                "error": None,
            }
        return None

    writer = _AutoResolveWriter(control, postback_builder)
    control.register_client("antikythera-client", writer)
    config = ToolLoopConfig(
        session_id="hook-override-server", max_steps=5, prompts=["decide for me"],
        provider="stub", model="stub", runtime_hooks_enabled=True,
        client_id="antikythera-client", pending_ttl_secs=10.0,
    )
    rt = make_runtime()
    outcome = run_tool_loop(rt, registry, gate, lambda name: stub, control, config)

    assert outcome.action == "final"
    assert outcome.steps == 1
    assert outcome.content == "client-hook-decision", (
        "override decide-action dari client harus dikomit sebagai final content"
    )
    hooks = [_parse_frame(f) for f in writer.frames if b"hook-request" in f]
    assert hooks, "harus ada frame hook-request di SSE"
    for hook_request in hooks:
        assert set(hook_request) == ENVELOPE_KEYS
        assert set(hook_request["payload"]) == {"hook", "session_state_json", "input_json"}
    assert any(h["payload"]["hook"] == "prepare-turn" for h in hooks)
    assert any(h["payload"]["hook"] == "decide-action" for h in hooks)
    assert control.pending_len() == 0


def test_hook_gate_denial_permission_prefix(make_runtime, wasmtime):
    """Hook tidak di-allowlist → denial gate meluas dari panggilan runner
    dengan prefix `permission:` (fail-closed: hook gagal = abort, tidak pernah
    fallback ke passthrough)."""
    stub = _ScriptedStub(['{"action": "final", "content": "unreachable"}'])
    gate = PolicyGate()  # default-deny: tidak ada hook yang di-allow
    control = ControlChannel(keepalive_interval=3600.0)
    control.register_client("antikythera-client", lambda data: None)
    config = ToolLoopConfig(
        session_id="hook-denied", max_steps=5, prompts=["hi"],
        provider="stub", model="stub", runtime_hooks_enabled=True,
        client_id="antikythera-client", pending_ttl_secs=10.0,
    )
    rt = make_runtime()
    with pytest.raises(Exception) as excinfo:
        run_tool_loop(rt, UnionRegistry(), gate, lambda name: stub, control, config)
    assert "permission:" in str(excinfo.value), f"denial hook harus berprefix permission: {excinfo.value}"


def test_hook_disabled_never_consults_peer(make_runtime, wasmtime):
    """`runtime_hooks_enabled=false` (default): runner TIDAK pernah memanggil
    runtime-hooks → tidak ada hook-request di SSE dan provider host tidak
    disentuh (loop berjalan tanpa peer)."""
    stub = _ScriptedStub(['{"action": "final", "content": "no-hooks"}'])
    control = ControlChannel(keepalive_interval=3600.0)
    writer = _AutoResolveWriter(control, lambda envelope: None)
    control.register_client("antikythera-client", writer)
    config = ToolLoopConfig(
        session_id="no-hooks", max_steps=5, prompts=["hi"],
        provider="stub", model="stub", runtime_hooks_enabled=False,
        client_id="antikythera-client", pending_ttl_secs=10.0,
    )
    rt = make_runtime()
    outcome = run_tool_loop(rt, UnionRegistry(), PolicyGate(), lambda name: stub, control, config)

    assert outcome.action == "final"
    assert outcome.content == "no-hooks"
    assert not any(b"hook-request" in f for f in writer.frames)


# ===========================================================================
# 7. Error path — max_steps / retry / action tak dikenal
# ===========================================================================

def test_max_steps_exceeded_errors(make_runtime, wasmtime):
    """max_steps terlampaui tanpa `final` → ToolLoopError dengan pesan mirror
    Rust `max_steps (N) exceeded without final action`."""
    # Stub selalu call_tool (echo builtin in-band, tanpa routing) — loop
    # tidak pernah mencapai final; iterasi LLM = max_steps lalu error.
    stub = _ScriptedStub(['{"action": "call_tool", "tool": "echo", "input": {}}'])
    config = ToolLoopConfig(
        session_id="max-steps-loop", max_steps=3, prompts=["go"],
        provider="stub", model="stub",
    )
    rt = make_runtime()
    with pytest.raises(ToolLoopError) as excinfo:
        run_tool_loop(rt, UnionRegistry(), PolicyGate(), lambda name: stub, _UnusedControl(), config)
    assert str(excinfo.value) == "tool loop: max_steps (3) exceeded without final action"


def test_retry_action_errors(make_runtime, wasmtime):
    """Aksi `retry` dari runner → error eksplisit berisi pesan retry."""
    stub = _ScriptedStub(['{"action": "retry", "error": "llm confused"}'])
    config = ToolLoopConfig(
        session_id="retry-loop", max_steps=5, prompts=["go"],
        provider="stub", model="stub",
    )
    rt = make_runtime()
    with pytest.raises(ToolLoopError) as excinfo:
        run_tool_loop(rt, UnionRegistry(), PolicyGate(), lambda name: stub, _UnusedControl(), config)
    assert str(excinfo.value) == "tool loop: runner requested retry: llm confused"


def test_unknown_action_errors(make_runtime, wasmtime):
    """Aksi tak dikenal → error eksplisit dari commit runner (`Unknown action`)
    sebelum loop berlanjut — tidak ada degradasi senyap."""
    stub = _ScriptedStub(['{"action": "hibernate"}'])
    config = ToolLoopConfig(
        session_id="unknown-action", max_steps=5, prompts=["go"],
        provider="stub", model="stub",
    )
    rt = make_runtime()
    with pytest.raises(Exception) as excinfo:
        run_tool_loop(rt, UnionRegistry(), PolicyGate(), lambda name: stub, _UnusedControl(), config)
    assert "Unknown action" in str(excinfo.value)
    assert "hibernate" in str(excinfo.value)


def test_unknown_action_via_hook_override_errors(make_runtime, wasmtime):
    """Branch defensif loop `unknown action` TETAP terjangkau: override hook
    `decide-action` boleh membawa action sembarang (CommitResult serde menerima
    string apapun) → commit sukses dengan action tak dikenal → loop gagal
    eksplisit `unknown action '{action}'` (mirror branch Rust loop_owner)."""
    stub = _ScriptedStub(['{"action": "final", "content": "server-default"}'])
    gate = PolicyGate()
    gate.allow_hook("prepare-turn")
    gate.allow_hook("decide-action")
    control = ControlChannel(keepalive_interval=3600.0)

    def postback_builder(envelope: Dict[str, Any]):
        if envelope["type"] == "hook-request" and envelope["payload"]["hook"] == "decide-action":
            return {
                "correlation_id": envelope["correlation_id"],
                "ok": True,
                "payload": {"action": "hibernate"},
                "error": None,
            }
        if envelope["type"] == "hook-request":
            return {
                "correlation_id": envelope["correlation_id"],
                "ok": True,
                "payload": {"passthrough": True},
                "error": None,
            }
        return None

    control.register_client("antikythera-client", _AutoResolveWriter(control, postback_builder))
    config = ToolLoopConfig(
        session_id="unknown-action-hook", max_steps=5, prompts=["go"],
        provider="stub", model="stub", runtime_hooks_enabled=True,
        client_id="antikythera-client", pending_ttl_secs=10.0,
    )
    rt = make_runtime()
    with pytest.raises(ToolLoopError) as excinfo:
        run_tool_loop(rt, UnionRegistry(), gate, lambda name: stub, control, config)
    assert str(excinfo.value) == "tool loop: unknown action 'hibernate'"
