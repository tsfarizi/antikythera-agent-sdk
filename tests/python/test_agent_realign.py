"""Falsifikasi test untuk realign `Agent`/`Orchestrator` terhadap ekspor WASM runner.

Konteks cacat (audit T2): `python/antikythera_agent/agent.py:97` memanggil
`self._runtime.call_checked("agent_run", args)` dan
`python/antikythera_agent/orchestrator.py:136` memanggil
`call_checked("orchestrator_dispatch", args)`; baris 216 memanggil
`call("orchestrator_cancel", "{}")`.

Ekspor runner yang BENAR (16 fungsi, lihat `wit/antikythera.wit` interface
`runner` dan `antikythera-sdk/src/wasm_exports.rs`) TIDAK memuat
`agent-run`, `orchestrator-dispatch`, maupun `orchestrator-cancel`.
`WasmRuntime.call` menormalkan snake_case->kebab-case
(`func_name.replace("_", "-")`), sehingga `agent_run`->`agent-run` selalu
raise ``WasmRuntimeError("Export 'agent_run' not found in WASM")`` dan
`Agent.run` selalu mengembalikan ``AgentResult(success=False, error=...)``.

Suite ini memfalsifikasi klausa kontrak berikut:
1. Setiap panggilan WASM dari `Agent`/`Orchestrator` menunjuk nama ekspor
   runner yang SAH (salah satu dari 16 ekspor `runner`), tidak ada panggilan
   ke `agent-run`/`orchestrator-dispatch`/`orchestrator-cancel`.
2. `Agent.run` meng-parse respons mock menjadi `AgentResult`
   (output/success/steps_used/session_id); error path -> success=False.
3. `Orchestrator.dispatch` meng-parse respons mock menjadi `TaskResult`;
   error path -> success=False. `Orchestrator.cancel` tidak menembak ekspor
   terlarang dan tidak melempar.
4. API publik Python (konstruktor, `from_config`, `run`/`dispatch`,
   tipe `AgentResult`/`TaskResult` dari `types.py`) dipertahankan.

Status pasca realign E1 (delegasi penuh): `agent.py` tidak lagi memuat
panggilan runner apa pun; seluruh panggilan ekspor WASM tingkat Agent kini
dimiliki `antikythera_agent/local_loop.py::run_local_loop`. Klausa statis
source-contract Agent diganti tiga bentuk baru: (i) source `agent.py` NOL
panggilan `call`/`call_checked` langsung sama sekali; (ii) semua nama
ekspor WASM yang dipakai tingkat Agent berada di `local_loop.py` dan
semuanya anggota 16 ekspor valid WIT; (iii) `agent.py` mengimpor
`run_local_loop` dari `local_loop`.

Deviasi harness yang dideklarasikan (temuan mekanis pasca-E1, bukan
penyimpangan klausa): port lama `call_checked` tidak lagi dilalui `Agent`,
sehingga mock agent-level diperbarui ke protokol runner loop (`init` ->
`prepare_user_turn` -> `commit_llm_response` -> `drain_events`) pada port
`call`, resolusi LLM dikontrol lewat kwarg publik `provider_resolver`
(amanemen kontrak E1), override FailingMock Agent pindah
`call_checked`->`call`, dan `steps_used` happy-path Agent menjadi 1
(satu iterasi final; `Agent.run` selalu memakai `registry=None` sehingga
putaran tool tidak tersedia di unit ini). Klaim tiap test behavioral
(subset-valid, tanpa nama terlarang, parsing `AgentResult`/`TaskResult`,
error path infallible S6) dipertahankan apa adanya.

Metode: MOCK `WasmRuntime` (substitusi instance pada port `_runtime`). Tidak
ada komposit/wasmtime yang dibutuhkan; respons shape JSON dihasilkan oleh
mock sesuai kontrak ekspor. Hanya test yang ditulis — `agent.py`,
`orchestrator.py`, `runtime.py` TIDAK diubah.

Direktori paket python (induk dari antikythera_agent/).

Pasca-E1: pemilik tunggal panggilan ekspor WASM tingkat Agent.

16 ekspor runner yang SAH (dari `wit/antikythera.wit` interface `runner` dan
`antikythera-sdk/src/wasm_exports.rs`).

Nama panggilan WASM yang TERLARANG (tidak ada pada ekspor runner).

Normalisasi snake_case->kebab-case yang dilakukan `WasmRuntime.call`.

FIXTURE / MOCK — port ke WasmRuntime, merekam nama fungsi yang dipanggil.

1. STATIC ASSERTION — klausa source-contract pasca-E1: (i) agent.py nol panggilan runner
langsung; (iii) agent.py mengimpor run_local_loop dari local_loop; (ii) seluruh nama ekspor
WASM tingkat Agent (kini milik local_loop.py) adalah anggota 16 ekspor runner sah.
Orchestrator tetap: hanya ekspor sah yang dipanggil.

2. BEHAVIOR TEST — mock runtime merekam nama fungsi yang dipanggil.

3. API PUBLIK — realign tidak mengubah kontrak publik Python.
"""

from __future__ import annotations

import json
import re
from pathlib import Path

import pytest

from antikythera_agent import Agent, Orchestrator
from antikythera_agent.agent import Agent as AgentClass
from antikythera_agent.orchestrator import Orchestrator as OrchestratorClass
from antikythera_agent.runtime import WasmRuntimeError
from antikythera_agent.server.provider import StubProvider
from antikythera_agent.types import (
    AgentConfig,
    AgentProfileConfig,
    AgentResult,
    OrchestratorConfig,
    PipelineResult,
    TaskResult,
)

_PYTHON_DIR = (Path(__file__).resolve().parents[2] / "clients" / "python")
_AGENT_SOURCE = _PYTHON_DIR / "antikythera_agent" / "agent.py"
_ORCHESTRATOR_SOURCE = _PYTHON_DIR / "antikythera_agent" / "orchestrator.py"
_LOCAL_LOOP_SOURCE = _PYTHON_DIR / "antikythera_agent" / "local_loop.py"

VALID_RUNNER_EXPORTS = {
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
}

FORBIDDEN_CALLS = {"agent_run", "agent-run", "orchestrator_dispatch",
                   "orchestrator-dispatch", "orchestrator_cancel",
                   "orchestrator-cancel"}

def _normalize(func_name: str) -> str:
    return func_name.replace("_", "-")


class MockRuntime:
    """Pengganti `WasmRuntime` yang merekam panggilan dan mengembalikan
    shape JSON respons sesuai kontrak ekspor runner.

    Mensimulasikan: `call` mengembalikan string JSON; `call_checked`
    mengembalikan dict hasil parse. Untuk nama yang TIDAK termasuk ekspor
    runner sah, mock meniru `WasmRuntime.call` dengan melempar
    `WasmRuntimeError("Export '<snake>' not found in WASM")` — sehingga
    behavior test memperlakukan panggilan terlarang persis seperti runtime
    nyata, sekaligus merekamnya untuk asersi subset-valid.

    Pasca-E1 (deviasi harness terdeklarasi): port `call` juga mensimulasikan
    protokol runner yang kini dimiliki `local_loop.run_local_loop` —
    `init` -> session id, `prepare_user_turn` -> prepared turn,
    `commit_llm_response` -> aksi final bercontent dari `agent_response`,
    `drain_events` -> daftar event kosong. Nama lain mempertahankan fallback
    lama (`json.dumps(task_response)`) sehingga jalur orchestrator
    (`get_state`, `reset_session`, `call_checked("init")`) tidak berubah.
    """

    def __init__(self, agent_response: dict | None = None,
                 task_response: dict | None = None) -> None:
        """Nama fungsi yang dipanggil (mentah).

        """
        self.calls: list[str] = []
        self.agent_response = agent_response or {
            "output": "mock agent output",
            "success": True,
            "steps_used": 3,
            "session_id": "sess-agent-1",
            "error": None,
        }
        self.task_response = task_response or {
            "task_id": "task-1",
            "agent_id": "coder",
            "output": "mock task output",
            "success": True,
            "steps_used": 4,
            "session_id": "sess-task-1",
            "error": None,
            "error_kind": None,
            "duration_ms": 12,
        }

    def call(self, func_name: str, args: str) -> str:
        """Protokol runner local_loop pasca-E1 (port `call`).

        Fallback lama: shape task untuk panggilan non-loop.
        """
        self.calls.append(func_name)
        if _normalize(func_name) not in VALID_RUNNER_EXPORTS:
            raise WasmRuntimeError(
                f"Export '{func_name}' not found in WASM")
        if func_name == "init":
            return self.agent_response["session_id"]
        if func_name == "prepare_user_turn":
            return json.dumps({"messages_json": "[]"})
        if func_name == "commit_llm_response":
            return json.dumps({
                "action": "final",
                "content": self.agent_response.get("output"),
            })
        if func_name == "drain_events":
            return "[]"
        return json.dumps(self.task_response)

    def call_checked(self, func_name: str, args: str) -> dict:
        self.calls.append(func_name)
        if _normalize(func_name) not in VALID_RUNNER_EXPORTS:
            raise WasmRuntimeError(
                f"Export '{func_name}' not found in WASM")
        return self.agent_response


@pytest.fixture
def mock_runtime() -> MockRuntime:
    return MockRuntime()


def _install_mock(instance, mock: MockRuntime) -> MockRuntime:
    """Pasang mock pada port `_runtime` unit dan kembalikan referensinya."""
    instance._runtime = mock
    return mock


def _extract_wasm_calls(source: str) -> list[str]:
    """Ekstrak nama fungsi yang diteruskan ke `call(` / `call_checked(`."""
    pattern = re.compile(
        r"\.(?:call|call_checked)\(\s*[\"']([^\"']+)[\"']\s*[,)]"
    )
    return pattern.findall(source)


def _extract_import_names(source: str, module: str) -> list[str]:
    """Ekstrak nama yang diimpor `from <module> import ...` (baris tunggal,
    dukung kurung satu baris dan alias `as`)."""
    pattern = re.compile(
        rf"from\s+{re.escape(module)}\s+import\s+\(?([^\n()]+?)\)?\s*$",
        re.MULTILINE,
    )
    names: list[str] = []
    for match in pattern.finditer(source):
        for piece in match.group(1).split(","):
            name = piece.split(" as ")[0].strip()
            if name:
                names.append(name)
    return names


def test_source_agent_does_not_call_forbidden_agent_run():
    """agent.py TIDAK memanggil `agent_run` sebagai nama fungsi WASM."""
    source = _AGENT_SOURCE.read_text(encoding="utf-8")
    assert "agent_run" not in source, (
        "agent.py masih memanggil ekspor WASM yang tidak ada: 'agent_run'")


def test_source_agent_has_no_direct_wasm_runtime_calls():
    """Klausa (i): pasca-E1 agent.py NOL panggilan `runtime.call`/
    `call_checked` langsung sama sekali — delegasi penuh ke
    `local_loop.run_local_loop`.


    Ketat: bentuk sintaktik apa pun (termasuk argumen non-literal) juga dilarang — pemanggilan
    via variabel akan lolos ekstraksi literal.
    """
    source = _AGENT_SOURCE.read_text(encoding="utf-8")
    calls = _extract_wasm_calls(source)
    assert not calls, (
        f"agent.py masih memanggil runner secara langsung (literal): {calls} "
        "(pasca-E1 semua panggilan ekspor harus dimiliki local_loop)")
    assert ".call(" not in source and ".call_checked(" not in source, (
        "agent.py memuat bentuk panggilan `.call(`/`.call_checked(` langsung "
        "(pasca-E1 harus nol)")


def test_source_local_loop_all_wasm_calls_are_valid_exports():
    """Klausa (ii): semua nama ekspor WASM yang dipakai tingkat Agent —
    pasca-E1 seluruhnya di local_loop.py — adalah anggota 16 ekspor sah."""
    source = _LOCAL_LOOP_SOURCE.read_text(encoding="utf-8")
    calls = _extract_wasm_calls(source)
    assert calls, "local_loop.py tidak memanggil fungsi WASM apa pun"
    normalized = {_normalize(c) for c in calls}
    assert normalized <= VALID_RUNNER_EXPORTS, (
        f"local_loop.py memanggil ekspor WASM tidak sah: "
        f"{sorted(normalized - VALID_RUNNER_EXPORTS)}")


def test_source_agent_imports_run_local_loop_from_local_loop():
    """Klausa (iii): agent.py mengimpor `run_local_loop` dari
    `antikythera_agent.local_loop` — rantai delegasi E1 terhubung."""
    source = _AGENT_SOURCE.read_text(encoding="utf-8")
    imported = _extract_import_names(source, "antikythera_agent.local_loop")
    assert "run_local_loop" in imported, (
        "agent.py tidak mengimpor 'run_local_loop' dari "
        "'antikythera_agent.local_loop' — delegasi E1 terputus")


def test_source_orchestrator_does_not_call_forbidden_exports():
    """orchestrator.py TIDAK memanggil `orchestrator_dispatch` / `_cancel`."""
    source = _ORCHESTRATOR_SOURCE.read_text(encoding="utf-8")
    for forbidden in ("orchestrator_dispatch", "orchestrator_cancel"):
        assert forbidden not in source, (
            f"orchestrator.py masih memanggil ekspor WASM yang tidak ada: "
            f"'{forbidden}'")


def test_source_orchestrator_all_wasm_calls_are_valid_exports():
    """Setiap `call`/`call_checked` di orchestrator.py menunjuk ekspor sah."""
    source = _ORCHESTRATOR_SOURCE.read_text(encoding="utf-8")
    calls = _extract_wasm_calls(source)
    normalized = {_normalize(c) for c in calls}
    assert normalized <= VALID_RUNNER_EXPORTS, (
        f"orchestrator.py memanggil ekspor WASM tidak sah: "
        f"{sorted(normalized - VALID_RUNNER_EXPORTS)}")


def test_agent_run_calls_only_valid_exports_and_parses_result(mock_runtime):
    """Agent.run -> semua nama fungsi terekam termasuk ekspor sah, TIDAK
    termasuk agent-run; AgentResult di-parse dari respons mock.

    Pasca-E1: delegasi penuh ke `run_local_loop` — resolusi LLM dikontrol
    via kwarg publik `provider_resolver` (seam amanemen E1), dan
    `steps_used` faktual = 1 (satu iterasi final; `Agent.run` memakai
    `registry=None` sehingga putaran tool tidak tersedia).


    Content diabaikan; commit dari mock runner.

    Parses respons mock ke AgentResult.
    """
    stub = StubProvider("{}")
    agent = Agent(provider="openai", model="gpt-4o",
                  provider_resolver=lambda name: stub)
    _install_mock(agent, mock_runtime)

    result = agent.run("hi")

    assert isinstance(result, AgentResult)
    assert mock_runtime.calls, "Agent.run tidak memanggil runtime sama sekali"
    recorded = {_normalize(c) for c in mock_runtime.calls}
    assert recorded <= VALID_RUNNER_EXPORTS, (
        f"Agent.run memanggil ekspor WASM tidak sah: "
        f"{sorted(recorded - VALID_RUNNER_EXPORTS)}")
    assert "agent-run" not in recorded

    assert result.success is True
    assert result.output == "mock agent output"
    assert result.steps_used == 1
    assert result.session_id == "sess-agent-1"
    assert result.error is None


def test_agent_run_error_path_returns_success_false():
    """Agent.run error path -> AgentResult(success=False, error=...)."""
    class FailingMock(MockRuntime):
        def call(self, func_name, args):
            """Pasca-E1: loop memakai port `call`.

            """
            self.calls.append(func_name)
            raise WasmRuntimeError(f"Export '{func_name}' not found in WASM")

    agent = Agent(provider="openai", model="gpt-4o")
    failing = FailingMock()
    _install_mock(agent, failing)

    result = agent.run("hi")

    assert isinstance(result, AgentResult)
    assert result.success is False
    assert result.error is not None
    assert "not found in WASM" in result.error


def test_orchestrator_dispatch_calls_only_valid_exports_and_parses_result(
        mock_runtime):
    """Orchestrator.dispatch -> nama fungsi terekam termasuk ekspor sah,
    TIDAK termasuk orchestrator-dispatch; TaskResult di-parse.


    Parses respons mock ke TaskResult.
    """
    orch = Orchestrator(execution_mode="auto")
    orch.register_agent(AgentProfileConfig(
        id="coder", name="Coder", role="developer",
        system_prompt="You are a coder."))
    _install_mock(orch, mock_runtime)

    result = orch.dispatch("task")

    assert isinstance(result, TaskResult)
    assert mock_runtime.calls, "dispatch tidak memanggil runtime"
    recorded = {_normalize(c) for c in mock_runtime.calls}
    assert recorded <= VALID_RUNNER_EXPORTS, (
        f"Orchestrator.dispatch memanggil ekspor WASM tidak sah: "
        f"{sorted(recorded - VALID_RUNNER_EXPORTS)}")
    assert "orchestrator-dispatch" not in recorded

    assert result.success is True
    assert result.task_id == "task-1"
    assert result.agent_id == "coder"
    assert result.output == "mock task output"
    assert result.steps_used == 4
    assert result.session_id == "sess-task-1"
    assert result.duration_ms == 12


def test_orchestrator_dispatch_error_path_returns_success_false():
    """Orchestrator.dispatch error path -> TaskResult(success=False)."""
    class FailingMock(MockRuntime):
        def call_checked(self, func_name, args):
            self.calls.append(func_name)
            raise WasmRuntimeError(f"Export '{func_name}' not found in WASM")

    orch = Orchestrator(execution_mode="auto")
    _install_mock(orch, FailingMock())

    result = orch.dispatch("task")

    assert isinstance(result, TaskResult)
    assert result.success is False
    assert result.error is not None


def test_orchestrator_cancel_does_not_call_forbidden_export(mock_runtime):
    """Orchestrator.cancel TIDAK menembak orchestrator-cancel dan tidak
    melempar ketika runtime berhasil."""
    orch = Orchestrator(execution_mode="auto")
    _install_mock(orch, mock_runtime)

    orch.cancel()

    recorded = {_normalize(c) for c in mock_runtime.calls}
    assert "orchestrator-cancel" not in recorded


def test_agent_public_api_preserved(mock_runtime):
    """Konstruktor, from_config, run -> AgentResult tetap ada."""
    a1 = AgentClass(provider="openai", model="gpt-4o")
    assert isinstance(a1, AgentClass)
    assert isinstance(a1.get_config(), AgentConfig)

    cfg = AgentConfig(provider="anthropic", model="claude-3-opus")
    a2 = AgentClass.from_config(cfg)
    assert isinstance(a2, AgentClass)
    _install_mock(a2, mock_runtime)
    assert isinstance(a2.run("hi"), AgentResult)


def test_orchestrator_public_api_preserved(mock_runtime):
    """Konstruktor, from_config, register_agent, dispatch -> TaskResult,
    cancel, dispatch_many, pipeline, list_agents, get_budget tetap ada.


    Dispatch_many, pipeline tetap eksis; cancel tidak melempar.
    """
    o1 = OrchestratorClass(execution_mode="auto")
    assert isinstance(o1, OrchestratorClass)

    o2 = OrchestratorClass.from_config(OrchestratorConfig(execution_mode="auto"))
    assert isinstance(o2, OrchestratorClass)

    o2.register_agent(AgentProfileConfig(
        id="coder", name="Coder", role="developer",
        system_prompt="You are a coder."))
    assert len(o2.list_agents()) == 1
    assert o2.get_budget()["consumed_steps"] == 0

    _install_mock(o2, mock_runtime)
    assert isinstance(o2.dispatch("task"), TaskResult)

    _install_mock(o2, MockRuntime())
    assert len(o2.dispatch_many(["a", "b"])) == 2
    assert isinstance(o2.pipeline(["a", "b"]), PipelineResult)
    o2.cancel()
