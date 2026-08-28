"""Falsification suite UP3 — fase RED untuk engine `Orchestrator` faktual.

Sumber kebenaran:
- debug/U1-design-notes.md E2 — BudgetSnapshot faktual (consumed_steps/
  dispatched_tasks dari hasil engine, bukan konstanta), semantik cancel
  idempoten per-session_id, dispatch_many ThreadPoolExecutor dengan isolasi
  WasmRuntime per worker + preservasi urutan input, tabel mapping error_kind
  (E2.4), pipeline short-circuit saat task gagal.
- AMANEMEN KONTRAK E2 (keputusan orkestrator): `Orchestrator.__init__`
  menerima kwarg opsional `runtime_factory=None` (callable -> WasmRuntime baru
  per worker; default WasmRuntime).
- Signature `dispatch_many(tasks, session_ids=None)` dan tabel worker per
  execution_mode mengikuti E2.3.

Klausa yang difalsifikasi (satu klaim per test):
- (a) budget faktual: setelah 2 dispatch sukses -> get_budget()
      ['dispatched_tasks']==2, consumed_steps>0, exhausted False.
- (b) cancel(session_id)->True; cancel ulang id sama->False; cancel() tanpa
      arg me-reset semua session orchestrator (idempoten).
- (c) dispatch_many concurrent (max_concurrent_tasks=2, 4 task): hasil
      terurut sesuai input, semua sukses, bukti konkurensi deterministik via
      stub threading.Barrier(2) -> max_concurrent>=2.
- (d1) provider.call raise LlmError('ollama down') -> error_kind=='transient'.
- (d2) denial gate 'permission:' -> error_kind=='permanent'.
- (e1) pipeline 2 task sukses: success True, total_steps>0, final_output =
      output task terakhir.
- (e2) kegagalan di tengah pipeline: success False, error terisi, iterasi
      berhenti (task berikutnya tidak dieksekusi).

Amplop test (asumsi yang dideklarasikan agar sertifikasi tetap sah):
- Stub-only TANPA jaringan: provider dikontrol lewat port publik S3
  (`server.provider.resolve_provider`) via monkeypatch - orchestrator pasca-
  UP3 wajib resolving default melalui port itu (mirror wiring E1.3); ini
  patch pada seam publik yang didokumentasikan, bukan struktur internal.
- Komposit NYATA untuk runtime: seam `runtime._WASM_PATH` (mirror
  test_loop_owner.py); `runtime_factory` disuntik hanya di klausa (c) yang
  mensubstansikan isolasi runtime per worker konkuren.
- Konkurensi deterministik anti-stuck: `_BarrierStub` memakai
  threading.Barrier(2) dengan timeout eksplisit 15 detik + lock penghitung;
  BrokenBarrierError ditelan terkontrol sehingga skenario sequential-pun
  selesai < 60s (di bawah watchdog 90s) dan gagal di assert max_concurrent,
  bukan hang.
- Status RED: orchestrator lama adalah stub (budget konstanta 0, cancel()
  tanpa arg, dispatch_many sequential palsu tanpa session_ids, error_kind
  selalu None) - test gagal pada klausa masing-masing; yang kebetulan hijau
  dicatat eksplisit di laporan.

Menjalankan (dari repo root):
    $env:PYTHONPATH="python"
    python -m pytest python/tests/test_up3_orchestrator_red.py -q --basetemp="$env:TEMP\\opencode\\pyt" -p no:cacheprovider
"""

from __future__ import annotations

import sys
import threading
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
from antikythera_agent.orchestrator import Orchestrator  # noqa: E402
from antikythera_agent.runtime import WasmRuntime  # noqa: E402
from antikythera_agent.server import provider as provider_mod  # noqa: E402

#: Komposit yang diuji: packaged default atau audited dist (identik).
_COMPOSITE_WASM = _REPO_ROOT / "dist" / "antikythera-sdk.wasm"
_PACKAGED_WASM = _PYTHON_SRC / "antikythera_agent" / "antikythera.wasm"


# ===========================================================================
# Fixtures — pola disalin dari test_loop_owner.py (real-composite)
# ===========================================================================

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


def _patch_composite(monkeypatch, composite_path: Path) -> None:
    """Arahkan konstruksi WasmRuntime default ke komposit nyata."""
    monkeypatch.setattr(runtime_mod, "_WASM_PATH", composite_path)


# ===========================================================================
# Stub — deterministik, tanpa jaringan
# ===========================================================================

class _ScriptedStub(provider_mod.LlmProvider):
    """Provider urutan content framework-generic; content terakhir berulang."""

    def __init__(self, responses: List[str]) -> None:
        self._responses = list(responses)
        self._index = 0

    def _call(self, request: Dict[str, Any]) -> Dict[str, Any]:
        index = self._index
        self._index += 1
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


class _FailingProvider(provider_mod.LlmProvider):
    """Provider yang selalu raise LlmError — path transient (E2.4)."""

    def __init__(self, message: str = "ollama down") -> None:
        self._message = message

    def _call(self, request: Dict[str, Any]) -> Dict[str, Any]:
        raise provider_mod.LlmError(self._message)


class _BarrierStub(provider_mod.LlmProvider):
    """Stub konkurensi-deterministik: setiap call masuk barier `parties`.

    Bukti konkurensi: `max_concurrent` hanya mencapai >= parties bila >= 2
    provider call benar-benar overlap. Anti-stuck: wait dibatasi timeout
    eksplisit; skenario sequential-pun selesai terbatas waktu dan kegagalannya
    terlokali di assert klausa (c).
    """

    def __init__(self, parties: int, timeout_secs: float = 15.0) -> None:
        self._barrier = threading.Barrier(parties)
        self._timeout = timeout_secs
        self._lock = threading.Lock()
        self._in_flight = 0
        self.max_concurrent = 0

    def _call(self, request: Dict[str, Any]) -> Dict[str, Any]:
        with self._lock:
            self._in_flight += 1
            if self._in_flight > self.max_concurrent:
                self.max_concurrent = self._in_flight
        try:
            try:
                self._barrier.wait(timeout=self._timeout)
            except threading.BrokenBarrierError:
                pass  # anti-stuck: lanjut terkontrol, gagal di assert
        finally:
            with self._lock:
                self._in_flight -= 1
        return {
            "content": '{"action": "final", "content": "ok"}',
            "model": request.get("model"),
            "session_id": request.get("session_id"),
            "message_json": None,
            "tokens_used": 4,
            "finish_reason": "stop",
            "raw_response_json": None,
        }


# ===========================================================================
# (a) Budget faktual — dihitung dari engine, bukan konstanta 0
# ===========================================================================

def test_up3_a_budget_faktual_setelah_dua_dispatch_sukses(composite_path, monkeypatch, wasmtime):
    """Klausa (a): dua dispatch sukses (stub default -> final) harus membuat
    get_budget() melapor dispatched_tasks==2, consumed_steps>0 (faktual dari
    steps engine), dan budget TIDAK exhausted (tanpa batas diset)."""
    _patch_composite(monkeypatch, composite_path)
    orch = Orchestrator()

    first = orch.dispatch("tugas satu")
    second = orch.dispatch("tugas dua")
    assert first.success is True, f"(a) dispatch pertama harus sukses: {first!r}"
    assert second.success is True, f"(a) dispatch kedua harus sukses: {second!r}"

    budget = orch.get_budget()
    assert budget["dispatched_tasks"] == 2, (
        f"(a) dispatched_tasks harus 2 (faktual): {budget!r}"
    )
    assert budget["consumed_steps"] > 0, (
        f"(a) consumed_steps harus > 0 (dihitung dari engine): {budget!r}"
    )
    assert budget["is_step_budget_exhausted"] is False, (
        f"(a) tanpa max_total_steps, step budget tak boleh exhausted: {budget!r}"
    )
    assert budget["is_task_budget_exhausted"] is False, (
        f"(a) tanpa max_total_tasks, task budget tak boleh exhausted: {budget!r}"
    )


# ===========================================================================
# (b) cancel idempoten — per session_id dan reset semua
# ===========================================================================

def test_up3_b_cancel_per_session_lalu_reset_semua_idempoten(composite_path, monkeypatch, wasmtime):
    """Klausa (b): cancel(session_id)->True sekali; cancel ulang id sama->
    False; cancel() tanpa arg me-reset semua session orchestrator (True bila
    ada yang dihapus, False bila sudah kosong)."""
    _patch_composite(monkeypatch, composite_path)
    orch = Orchestrator()
    orch.dispatch("tugas a", session_id="up3-sess-a")
    orch.dispatch("tugas b", session_id="up3-sess-b")

    assert orch.cancel("up3-sess-a") is True, (
        "(b) cancel(session_id aktif) harus True"
    )
    assert orch.cancel("up3-sess-a") is False, (
        "(b) cancel ulang session_id sama harus False (idempoten)"
    )
    assert orch.cancel() is True, (
        "(b) cancel() tanpa arg harus True saat masih ada session tersisa"
    )
    assert orch.cancel() is False, (
        "(b) cancel() tanpa arg kedua kali harus False (semua sudah dihapus)"
    )


# ===========================================================================
# (c) dispatch_many concurrent — urutan input + bukti konkurensi barier
# ===========================================================================

def test_up3_c_dispatch_many_concurrent_terurut_semua_sukses_bukti_konkurensi(
    composite_path, monkeypatch, make_runtime, wasmtime
):
    """Klausa (c): mode concurrent, max_concurrent_tasks=2, 4 task — hasil
    terurut sesuai input (session_ids dipetakan indeks), semua sukses, dan
    stub barir membuktikan >= 2 provider call benar-benar overlap."""
    barrier_stub = _BarrierStub(parties=2, timeout_secs=15.0)

    def _concurrent_resolver(name: Optional[str] = None):
        return barrier_stub

    monkeypatch.setattr(provider_mod, "resolve_provider", _concurrent_resolver)

    tasks = ["t0", "t1", "t2", "t3"]
    session_ids = ["ord-0", "ord-1", "ord-2", "ord-3"]
    orch = Orchestrator(
        execution_mode="concurrent",
        max_concurrent_tasks=2,
        runtime_factory=lambda: make_runtime(),
    )

    results = orch.dispatch_many(tasks, session_ids)

    assert [r.session_id for r in results] == session_ids, (
        f"(c) hasil harus terurut sesuai input: {[r.session_id for r in results]!r}"
    )
    failed = [r for r in results if not r.success]
    assert not failed, f"(c) semua task harus sukses: {failed!r}"
    assert barrier_stub.max_concurrent >= 2, (
        f"(c) bukti konkurensi: max_concurrent harus >= 2, "
        f"teramati {barrier_stub.max_concurrent}"
    )


# ===========================================================================
# (d) error_kind mapping — transient vs permanent (E2.4)
# ===========================================================================

def test_up3_d1_provider_failure_llm_error_map_ke_transient(composite_path, monkeypatch, wasmtime):
    """Klausa (d1): provider.call raise LlmError('ollama down') -> TaskResult
    dengan error_kind=='transient' (kegagalan transport layak retry)."""
    _patch_composite(monkeypatch, composite_path)
    monkeypatch.setattr(
        provider_mod, "resolve_provider", lambda name=None: _FailingProvider("ollama down")
    )
    orch = Orchestrator()

    result = orch.dispatch("apa saja")

    assert result.success is False, f"(d1) dispatch harus gagal: {result!r}"
    assert result.error_kind == "transient", (
        f"(d1) LlmError harus dipetakan error_kind='transient': "
        f"{result.error_kind!r} (error={result.error!r})"
    )


def test_up3_d2_gate_denial_permission_map_ke_permanent(composite_path, monkeypatch, wasmtime):
    """Klausa (d2): stub minta call_tool yang ditolak gate default-deny ->
    TaskResult dengan error_kind=='permanent' (allowlist miss, retry sia-sia)."""
    _patch_composite(monkeypatch, composite_path)
    denial_stub = _ScriptedStub(['{"action": "call_tool", "tool": "ghost_tool", "input": {}}'])
    monkeypatch.setattr(provider_mod, "resolve_provider", lambda name=None: denial_stub)
    orch = Orchestrator()

    result = orch.dispatch("pakai ghost_tool")

    assert result.success is False, f"(d2) dispatch harus gagal: {result!r}"
    assert result.error_kind == "permanent", (
        f"(d2) denial gate harus dipetakan error_kind='permanent': "
        f"{result.error_kind!r} (error={result.error!r})"
    )


# ===========================================================================
# (e) pipeline — chaining sukses & short-circuit saat gagal
# ===========================================================================

def test_up3_e1_pipeline_chaining_dua_task_sukses_final_output_task_terakhir(composite_path, monkeypatch, wasmtime):
    """Klausa (e1): pipeline 2 task dengan stub default (final) — success
    True, total_steps>0 (faktual), final_output == output task terakhir."""
    _patch_composite(monkeypatch, composite_path)
    orch = Orchestrator()

    outcome = orch.pipeline(["tugas satu", "tugas dua"])

    assert outcome.success is True, (
        f"(e1) pipeline dua task sukses harus True: error={outcome.error!r}, "
        f"results={outcome.results!r}"
    )
    assert outcome.total_steps > 0, (
        f"(e1) total_steps harus > 0 (faktual dari engine): {outcome.total_steps}"
    )
    assert len(outcome.results) == 2, (
        f"(e1) kedua task harus tereksekusi: {len(outcome.results)}"
    )
    assert outcome.final_output == outcome.results[-1].output, (
        f"(e1) final_output harus milik task terakhir: {outcome.final_output!r}"
    )


def test_up3_e2_pipeline_gagal_di_tengah_berhenti_error_terisi(composite_path, monkeypatch, wasmtime):
    """Klausa (e2): provider gagal -> pipeline short-circuit: success False,
    error terisi, iterasi BERHENTI (task kedua tidak dieksekusi)."""
    _patch_composite(monkeypatch, composite_path)
    monkeypatch.setattr(
        provider_mod, "resolve_provider", lambda name=None: _FailingProvider("backend down")
    )
    orch = Orchestrator()

    outcome = orch.pipeline(["tugas awal", "tugas lanjutan"])

    assert outcome.success is False, (
        f"(e2) pipeline dengan task gagal harus False: {outcome!r}"
    )
    assert outcome.error, f"(e2) error pipeline harus terisi: {outcome.error!r}"
    assert len(outcome.results) == 1, (
        f"(e2) iterasi harus berhenti di task gagal (tidak lanjut task kedua): "
        f"{len(outcome.results)} hasil"
    )
