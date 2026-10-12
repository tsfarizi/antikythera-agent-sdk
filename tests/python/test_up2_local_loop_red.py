r"""Falsification suite UP2 — fase RED untuk `local_loop.py` + realign `Agent`.

Sumber kebenaran:
- debug/U1-design-notes.md E1 — signature `LocalLoopConfig` / `LoopOutcome` /
  `run_local_loop(runtime, provider_resolver, registry, gate, config)`,
  urutan ekspor runner (init → register_tools → prepare → llm → commit →
  drain → process), error taxonomy S6.
- AMANEMEN KONTRAK E1 (keputusan orkestrator): `Agent.__init__` menerima kwarg
  opsional `provider_resolver=None` (default
  `server.provider.resolve_provider`) — port S2.
- Pola fixture disalin dari python/tests/test_loop_owner.py (real-composite:
  make_runtime/WasmRuntime + StubProvider custom response + UnionRegistry +
  PolicyGate) — tanpa mock shape WASM, tanpa jaringan.

Klausa yang difalsifikasi (satu klaim per test):
- (a) run_local_loop happy-path: stub final → LoopOutcome.action=='final',
      steps>=1, content=='hi', session_id non-empty.
- (b) Agent.run end-to-end via provider_resolver: run pertama success True,
      output=='hi', session_id truthy; run kedua session_id sama tetap sukses.
- (c) resolver miss di loop → AgentResult.success False dan
      'unknown LLM provider' in error — BUKAN raise (invarian infallible S6).
- (d) max_steps=0 dan loop tak-final → success False, 'max_steps' in error.
- (e) tool lokal owner server: handler terpanggil persis sekali, loop lanjut
      ke final 'done'.
- (f) gate denial default-deny: error berprefix 'permission:'.

Amplop test (asumsi yang dideklarasikan agar sertifikasi tetap sah):
- Komposit NYATA + wasmtime (dep opsional) di-importorskip/skip keras dengan
  alasan eksplisit — mirror test_loop_owner.py; seam `runtime._WASM_PATH`.
- Stub LLM deterministik: `StubProvider` statis + `_ScriptedStub` urutan
  (disalin dari test_loop_owner.py); tanpa jaringan.
- Deviasi harness yang dideklarasikan (temuan mekanis, bukan penyimpangan
  klausa):
  * Envelope aksi stub memakai kunci TERBUKTI komposit `"tool"`/`"input"`
    (processor.rs:105-118 hanya membaca action/tool/input; mirror
    `_call_then_final` test_loop_owner + ScriptedStub Rust) — bukan
    `"tool_name"/"tool_input"` seperti tertulis di amanemen.
  * Klausa (e) memakai nama tool `local_echo`, bukan `echo`: `echo` adalah
    BUILTIN in-band komposit yang eksekusi host handler-nya terbukti
    dilewati (test_loop_owner::test_builtin_in_band_no_host_execution),
    sehingga klausa "handler terpanggil" mustahil dibuktikan lewat builtin.
- Status RED: modul `antikythera_agent.local_loop` belum ada → test loop-level
  gagal terkontrol via `_require_local_loop`; test Agent-level gagal alami
  (TypeError kwarg `provider_resolver` / jalur lama tanpa resolver).

Menjalankan (dari repo root):
    $env:PYTHONPATH="python"
    python -m pytest python/tests/test_up2_local_loop_red.py -q --basetemp="$env:TEMP\opencode\pyt" -p no:cacheprovider

Fixtures — disalin dari test_loop_owner.py (real-composite).

Stub — disalin dari test_loop_owner.py (deterministik, tanpa jaringan).

(a) run_local_loop happy-path — stub final → LoopOutcome.

(b) Agent.run end-to-end via provider_resolver — dua run, sesi kontinu.

(c) resolver miss di loop → result gagal, BUKAN raise.

(d) max_steps=0 / loop tak-final → result gagal berisi 'max_steps'.

(e) tool lokal owner server — handler terpanggil, final 'done'.

(f) gate denial default-deny — error berprefix 'permission:'.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional

import pytest

_PYTHON_SRC = (Path(__file__).resolve().parents[2] / "clients" / "python")
_REPO_ROOT = Path(__file__).resolve().parents[2]

if str(_PYTHON_SRC) not in sys.path:
    sys.path.insert(0, str(_PYTHON_SRC))

from antikythera_agent import host  # noqa: E402
from antikythera_agent import runtime as runtime_mod  # noqa: E402
from antikythera_agent.agent import Agent  # noqa: E402
from antikythera_agent.runtime import WasmRuntime  # noqa: E402
from antikythera_agent.server import provider as provider_mod  # noqa: E402
from antikythera_agent.server.gate import PolicyGate  # noqa: E402
from antikythera_agent.server.registry import UnionRegistry  # noqa: E402

try:  # pragma: no cover - jalur hijau pasca-implementasi
    from antikythera_agent.local_loop import (  # noqa: E402
        LocalLoopConfig,
        LoopOutcome,
        run_local_loop,
    )

    _LOCAL_LOOP_AVAILABLE = True
except ImportError:
    _LOCAL_LOOP_AVAILABLE = False
    LocalLoopConfig = None  # type: ignore[assignment]
    LoopOutcome = None  # type: ignore[assignment]
    run_local_loop = None  # type: ignore[assignment]


def _require_local_loop(clause: str) -> None:
    """Gagalkan test loop-level secara terkontrol selama modul belum ada."""
    if not _LOCAL_LOOP_AVAILABLE:
        pytest.fail(
            f"RED UP2 [{clause}]: modul antikythera_agent.local_loop belum ada "
            f"(run_local_loop/LocalLoopConfig/LoopOutcome belum diekspor) — "
            f"klausa {clause} belum dapat difalsifikasi lebih dalam."
        )


#: Komposit yang diuji: packaged default atau audited dist (identik).
_COMPOSITE_WASM = _REPO_ROOT / "dist" / "antikythera-sdk.wasm"
_PACKAGED_WASM = _PYTHON_SRC / "antikythera_agent" / "antikythera.wasm"



@pytest.fixture(scope="module")
def wasmtime():
    """wasmtime module, atau skip keras bila dep opsional tidak ada."""
    pytest.importorskip(
        "wasmtime",
        reason="wasmtime not installed; install with: pip install antikythera-agent[wasm] ",
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



class _ScriptedStub(provider_mod.LlmProvider):
    """Provider urutan content framework-generic; content terakhir berulang."""

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


def _call_then_final(tool: str, input_: Dict[str, Any]) -> List[str]:
    """Script dua langkah: call `tool` sekali, lalu final (mirror Rust)."""
    return [
        json.dumps({"action": "call_tool", "tool": tool, "input": input_}),
        json.dumps({"action": "final", "content": "done"}),
    ]



def test_up2_a_run_local_loop_happy_path_stub_final_menghasilkan_loopoutcome_final(make_runtime, wasmtime):
    """Klausa (a): stub final satu langkah → LoopOutcome.action=='final',
    steps>=1, content=='hi', session_id non-empty."""
    _require_local_loop("(a)")
    stub = provider_mod.StubProvider('{"action": "final", "content": "hi"}')
    rt = make_runtime()
    config = LocalLoopConfig(
        session_id="up2-a-final",
        max_steps=5,
        provider="stub",
        model="stub-model",
        system_prompt=None,
        timeout=60000,
        prompts=["halo"],
    )
    outcome = run_local_loop(rt, lambda name: stub, UnionRegistry(), PolicyGate(), config)

    assert isinstance(outcome, LoopOutcome), (
        f"(a) run_local_loop harus mengembalikan LoopOutcome, bukan {type(outcome)!r}"
    )
    assert outcome.action == "final", f"(a) action harus 'final': {outcome.action!r}"
    assert outcome.steps >= 1, f"(a) steps harus >= 1 (faktual): {outcome.steps}"
    assert outcome.content == "hi", f"(a) content harus 'hi': {outcome.content!r}"
    assert outcome.session_id, "(a) session_id hasil loop harus non-empty"



def test_up2_b_agent_run_end_to_end_stub_final_dua_run_session_id_konsisten(composite_path, monkeypatch):
    """Klausa (b): Agent(provider, model, provider_resolver=...) end-to-end —
    run pertama success True, output=='hi', session_id truthy; run kedua pada
    instance yang sama sukses dengan session_id identik (kontinuitas sesi)."""
    monkeypatch.setattr(runtime_mod, "_WASM_PATH", composite_path)

    def _resolver(name: Optional[str]):
        return provider_mod.StubProvider('{"action": "final", "content": "hi"}')

    agent = Agent(provider="stub", model="m", provider_resolver=_resolver)
    first = agent.run("halo")

    assert first.success is True, f"(b) run pertama harus sukses: error={first.error!r}"
    assert first.output == "hi", f"(b) output harus 'hi': {first.output!r}"
    assert first.session_id, f"(b) session_id run pertama harus truthy: {first.session_id!r}"

    second = agent.run("halo lagi")
    assert second.success is True, (
        f"(b) run kedua pada sesi sama harus tetap sukses: error={second.error!r}"
    )
    assert second.session_id == first.session_id, (
        f"(b) run kedua harus memakai session_id yang sama: "
        f"{second.session_id!r} != {first.session_id!r}"
    )



def test_up2_c_resolver_miss_di_loop_menghasilkan_result_gagal_dengan_unknown_llm_provider(composite_path, monkeypatch):
    """Klausa (c): nama provider tak dikenal → resolver miss di tengah loop →
    AgentResult.success False dengan 'unknown LLM provider' in error — run
    TIDAK boleh raise (invarian infallible result, taxonomy S6).


    TIDAK boleh raise — bila raise, klausa patah.
    """
    monkeypatch.setattr(runtime_mod, "_WASM_PATH", composite_path)

    agent = Agent(provider="no-such-llm-provider", model="m")
    result = agent.run("hi")

    assert result.success is False, (
        f"(c) resolver miss harus menghasilkan success=False: {result!r}"
    )
    assert "unknown LLM provider" in (result.error or ""), (
        f"(c) error harus mengandung 'unknown LLM provider': {result.error!r}"
    )



def test_up2_d_max_steps_nol_dan_loop_tak_final_sukses_false_error_max_steps(composite_path, monkeypatch):
    """Klausa (d): (1) max_steps=0 — loop tak pernah berjalan → success False,
    error memuat 'max_steps'; (2) loop tak-final (stub selalu call_tool echo
    builtin in-band) sampai budget habis → success False, 'max_steps' in
    error. Keduanya result, bukan raise."""
    monkeypatch.setattr(runtime_mod, "_WASM_PATH", composite_path)

    def _resolver(name: Optional[str]):
        return provider_mod.StubProvider('{"action": "final", "content": "hi"}')

    zero = Agent(provider="stub", model="m", max_steps=0, provider_resolver=_resolver)
    r0 = zero.run("go")
    assert r0.success is False, f"(d) max_steps=0 harus gagal sebagai result: {r0!r}"
    assert "max_steps" in (r0.error or ""), (
        f"(d) error max_steps=0 harus memuat 'max_steps': {r0.error!r}"
    )

    never_final = _ScriptedStub(['{"action": "call_tool", "tool": "echo", "input": {}}'])
    bounded = Agent(provider="stub", model="m", max_steps=3, provider_resolver=lambda n: never_final)
    r3 = bounded.run("go")
    assert r3.success is False, f"(d) loop tak-final harus gagal sebagai result: {r3!r}"
    assert "max_steps" in (r3.error or ""), (
        f"(d) error loop tak-final harus memuat 'max_steps': {r3.error!r}"
    )



def test_up2_e_tool_lokal_server_handler_terpanggil_lalu_final_done(make_runtime, wasmtime):
    """Klausa (e): tool owner server terdaftar + di-allowlist; stub dua tahap
    call_tool → final. Handler lokal terpanggil persis sekali dengan input
    LLM, loop lanjut ke final 'done'.

    Catatan harness: nama tool `local_echo` (bukan `echo` — builtin in-band
    komposit; lihat amplop) dan envelope kunci `tool`/`input` (terbukti
    processor.rs:105-118).
    """
    _require_local_loop("(e)")
    stub = _ScriptedStub(_call_then_final("local_echo", {"x": 1}))
    handler_calls: List[Any] = []

    def local_echo_handler(args: Any) -> Dict[str, Any]:
        handler_calls.append(args)
        return {"echoed": args}

    registry = UnionRegistry()
    registry.register_server(
        {"name": "local_echo", "description": "echo lokal"}, handler=local_echo_handler
    )
    gate = PolicyGate()
    gate.allow_tool("server", "local_echo")
    config = LocalLoopConfig(
        session_id="up2-e-tool",
        max_steps=5,
        provider="stub",
        model="stub-model",
        system_prompt=None,
        timeout=60000,
        prompts=["pakai tool"],
    )
    rt = make_runtime()
    outcome = run_local_loop(rt, lambda name: stub, registry, gate, config)

    assert handler_calls == [{"x": 1}], (
        f"(e) handler lokal harus terpanggil persis sekali dengan {{'x': 1}}: {handler_calls!r}"
    )
    assert outcome.action == "final", f"(e) loop harus berakhir final: {outcome.action!r}"
    assert outcome.content == "done", f"(e) content akhir harus 'done': {outcome.content!r}"
    assert outcome.steps == 2, f"(e) steps faktual dua tahap: {outcome.steps}"



def test_up2_f_gate_denial_default_deny_error_berprefix_permission(make_runtime, wasmtime):
    """Klausa (f): tool terdaftar tapi TIDAK di-allowlist (gate default-deny);
    stub minta call_tool → denial sebelum eksekusi → error berprefix
    'permission:' (fail-closed R4), bukan eksekusi senyap.


    Denial boleh meluas sebagai exception loop.
    """
    _require_local_loop("(f)")
    stub = _ScriptedStub(_call_then_final("local_secret", {}))
    executed: List[Any] = []

    def must_not_run(args: Any) -> Dict[str, Any]:
        executed.append(args)
        return {}

    registry = UnionRegistry()
    registry.register_server(
        {"name": "local_secret", "description": "rahasia"}, handler=must_not_run
    )
    gate = PolicyGate()
    config = LocalLoopConfig(
        session_id="up2-f-denied",
        max_steps=5,
        provider="stub",
        model="stub-model",
        system_prompt=None,
        timeout=60000,
        prompts=["baca rahasia"],
    )
    rt = make_runtime()
    try:
        outcome = run_local_loop(rt, lambda name: stub, registry, gate, config)
    except Exception as exc:
        assert str(exc).startswith("permission:"), (
            f"(f) denial gate harus berprefix 'permission:': {exc!r}"
        )
    else:
        pytest.fail(
            f"(f) denial gate harus menghasilkan error 'permission:' — malah "
            f"kembali tanpa error: action={getattr(outcome, 'action', None)!r}, "
            f"handler_executed={executed!r}"
        )
    assert executed == [], f"(f) handler TIDAK boleh tereksekusi saat denial: {executed!r}"
