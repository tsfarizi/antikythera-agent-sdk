"""Multi-agent orchestrator implementation.

Engine UP3: dispatch menjalankan loop host-side (`antikythera_agent.local_loop`)
di atas `WasmRuntime` — satu runtime per sesi untuk `dispatch`, runtime baru
per task untuk `dispatch_many` (isolasi Store antar thread), budget faktual
dari hasil engine, dan `error_kind` mengikuti tabel E2.4 di
`debug/U1-design-notes.md`.

Amplop validitas:
- Satu sesi tidak boleh di-dispatch paralel dari beberapa thread (Store
  wasmtime tidak thread-safe); isolasi konkurensi disediakan lewat pemisahan
  sesi/runtime, bukan sinkronisasi satu Store.
- Resolusi provider default selalu melewati port publik
  `server.provider.resolve_provider` secara late-bound agar wiring/patch pada
  port itu berlaku untuk dispatch yang sudah berjalan (mirror E1.3).
"""

from __future__ import annotations

import json
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from typing import Any, Callable, Literal, Optional

from antikythera_agent.local_loop import (
    LocalLoopConfig,
    ToolLoopError,
    run_local_loop,
)
from antikythera_agent.runtime import WasmRuntime, WasmRuntimeError
from antikythera_agent.server import provider
from antikythera_agent.server.gate import PERMISSION_PREFIX, PermissionDeniedError
from antikythera_agent.types import (
    AgentProfileConfig,
    OrchestratorConfig,
    PipelineResult,
    TaskResult,
)

#: Batas langkah loop bila tidak ada profil terdaftar — mirror default
#: `AgentProfileConfig.max_steps`.
_DEFAULT_MAX_STEPS = 8

#: Timeout loop (ms) bila tidak ada profil terdaftar — mirror
#: `AgentConfig.timeout`.
_DEFAULT_TIMEOUT_MS = 60000

#: Marker pesan retry runner (`local_loop.run_local_loop`) untuk klasifikasi
#: transient pada error loop yang tidak membawa cause bertipe.
_RETRY_MARKER = "runner requested retry"

#: Marker pesan kehabisan langkah loop (`max_steps exceeded`) — permanent
#: menurut E2.4 (retry tanpa perubahan config sia-sia).
_MAX_STEPS_MARKER = "max_steps"


def _default_provider_resolver(name: Optional[str] = None) -> Any:
    """Resolusi late-bound lewat atribut modul port S3: referensi fungsi yang
    di-import statis tidak terpengaruh patch/wiring ulang pada port."""
    return provider.resolve_provider(name)


def _classify_loop_error(
    exc: BaseException,
) -> Optional[Literal["transient", "permanent", "cancelled"]]:
    """Peta exception loop -> `error_kind` tabel E2.4.

    Loop membungkus kegagalan internal menjadi `ToolLoopError(...) from exc`,
    sehingga tipe sumber dicari lewat rantai `__cause__`; pesan dipakai hanya
    sebagai fallback untuk varian `ToolLoopError` tanpa cause bertipe
    (denial routing, max_steps, retry). Return `None` bila tak ada baris tabel
    yang cocok — jenis baru tidak boleh direka (R7).
     """
    current: Optional[BaseException] = exc
    seen: set[int] = set()
    while current is not None and id(current) not in seen:
        seen.add(id(current))
        if isinstance(current, PermissionDeniedError):
            return "permanent"
        if isinstance(current, (provider.LlmError, WasmRuntimeError)):
            return "transient"
        if isinstance(current, ValueError):
            return "permanent"
        current = current.__cause__
    message = str(exc)
    if message.startswith(PERMISSION_PREFIX):
        return "permanent"
    if _MAX_STEPS_MARKER in message:
        return "permanent"
    if _RETRY_MARKER in message:
        return "transient"
    return None


class Orchestrator:
    """Multi-agent orchestrator for parallel and sequential task execution.

    Manages multiple agents, routes tasks, and coordinates execution through
    the local tool loop engine. Supports factual budget accounting, guardrail
    enforcement in `dispatch_many`, and cooperative per-session cancellation.

    Example:
        >>> orchestrator = Orchestrator(execution_mode="auto")
        >>> orchestrator.register_agent(
        ...     AgentProfileConfig(
        ...         id="coder",
        ...         name="Code Writer",
        ...         role="coder",
        ...         system_prompt="You are an expert programmer."
        ...     )
        ... )
        >>> result = orchestrator.dispatch("Write a sorting algorithm")
        >>> print(result.output)
    """

    def __init__(
        self,
        execution_mode: str = "auto",
        max_concurrent_tasks: int = 4,
        max_total_steps: Optional[int] = None,
        max_total_tasks: Optional[int] = None,
        default_retry_condition: str = "always",
        runtime_factory: Optional[Callable[[], WasmRuntime]] = None,
    ):
        """Create a new Orchestrator instance.

        Args:
            execution_mode: How tasks are executed ('auto', 'sequential', 'concurrent', 'parallel').
            max_concurrent_tasks: Maximum tasks running simultaneously.
            max_total_steps: Maximum total steps across all tasks.
            max_total_tasks: Maximum number of tasks.
            default_retry_condition: Retry policy ('always', 'on-transient', 'never').
            runtime_factory: Optional callable producing a fresh `WasmRuntime`
                (used per isolated worker in `dispatch_many`); defaults to
                constructing `WasmRuntime()` directly.
        """
        self._config = OrchestratorConfig(
            execution_mode=execution_mode,  # type: ignore
            max_concurrent_tasks=max_concurrent_tasks,
            max_total_steps=max_total_steps,
            max_total_tasks=max_total_tasks,
            default_retry_condition=default_retry_condition,  # type: ignore
        )
        self._agents: list[AgentProfileConfig] = []
        self._runtime_factory = runtime_factory
        self._runtime = self._make_runtime()
        # Guard state bersama lintas worker: peta sesi -> runtime dan dua
        # penghitung budget di-update atomik di bawah satu kunci.
        self._lock = threading.Lock()
        self._session_runtimes: dict[str, WasmRuntime] = {}
        self._consumed_steps = 0
        self._dispatched_tasks = 0

    @classmethod
    def from_config(cls, config: OrchestratorConfig) -> Orchestrator:
        """Create an Orchestrator from a config object.

        Args:
            config: Orchestrator configuration.

        Returns:
            Configured Orchestrator instance.
        """
        return cls(
            execution_mode=config.execution_mode,
            max_concurrent_tasks=config.max_concurrent_tasks,
            max_total_steps=config.max_total_steps,
            max_total_tasks=config.max_total_tasks,
            default_retry_condition=config.default_retry_condition,
        )

    def _make_runtime(self) -> WasmRuntime:
        """Bangun runtime baru: produk factory bila disuntikkan, else default."""
        if self._runtime_factory is not None:
            return self._runtime_factory()
        return WasmRuntime()

    def _active_profile(self) -> Optional[AgentProfileConfig]:
        """Profil agen pelaksana: registrasi pertama (urutan registrasi =
        prioritas routing), atau None bila belum ada profil."""
        return self._agents[0] if self._agents else None

    def register_agent(self, profile: AgentProfileConfig) -> None:
        """Register an agent profile.

        Args:
            profile: Agent profile configuration.

        Raises:
            ValueError: If profile is missing required fields.
        """
        if not profile.id or not profile.name or not profile.role:
            raise ValueError("Agent profile requires id, name, and role")
        self._agents.append(
            AgentProfileConfig(
                id=profile.id,
                name=profile.name,
                role=profile.role,
                system_prompt=profile.system_prompt,
                max_steps=profile.max_steps,
            )
        )

    def _acquire_session_runtime(self, session_id: str) -> WasmRuntime:
        """Runtime untuk satu sesi: reuse bila sesi sudah tercatat, else buat
        baru dan catat (di bawah kunci) agar `cancel` menjangkaunya."""
        with self._lock:
            runtime = self._session_runtimes.get(session_id)
            if runtime is None:
                runtime = self._make_runtime()
                self._session_runtimes[session_id] = runtime
            return runtime

    def _register_session(self, session_id: str, runtime: WasmRuntime) -> None:
        """Catat sesi aktual hasil engine (id dari runner) milik `runtime`."""
        if not session_id:
            return
        with self._lock:
            self._session_runtimes.setdefault(session_id, runtime)

    def _execute_on_runtime(
        self, task: str, session_id: Optional[str], runtime: WasmRuntime
    ) -> TaskResult:
        """Jalankan satu task pada `runtime` tertentu dan petakan hasil/error
        loop ke `TaskResult`; snapshot budget di-update atomik tiap hasil."""
        profile = self._active_profile()
        loop_config = LocalLoopConfig(
            session_id=session_id or "",
            max_steps=(
                profile.max_steps if profile is not None else _DEFAULT_MAX_STEPS
            ),
            provider=provider.DEFAULT_PROVIDER,
            model="",
            system_prompt=profile.system_prompt if profile is not None else None,
            timeout=_DEFAULT_TIMEOUT_MS,
            prompts=[task],
        )
        started = time.monotonic()
        try:
            outcome = run_local_loop(runtime, _default_provider_resolver, None, None, loop_config)
        except (
            ToolLoopError,
            provider.LlmError,
            WasmRuntimeError,
            PermissionDeniedError,
            ValueError,
        ) as exc:
            result = TaskResult(
                task_id="",
                agent_id=profile.id if profile is not None else "",
                output=None,
                success=False,
                steps_used=0,
                session_id=session_id or "",
                error=str(exc),
                error_kind=_classify_loop_error(exc),
                duration_ms=int((time.monotonic() - started) * 1000),
            )
        else:
            result = TaskResult(
                task_id="",
                agent_id=profile.id if profile is not None else "",
                output=outcome.content,
                success=True,
                steps_used=outcome.steps,
                session_id=outcome.session_id,
                duration_ms=int((time.monotonic() - started) * 1000),
            )
            self._register_session(outcome.session_id, runtime)
        with self._lock:
            self._consumed_steps += result.steps_used
            self._dispatched_tasks += 1
        return result

    def _dispatch_via_external_backend(
        self, task: str, session_id: Optional[str]
    ) -> TaskResult:
        """Jembatan kompatibilitas untuk backend orkestrasi eksternal yang
        dipasangkan langsung pada port `_runtime` (kontrak lama: state JSON
        dari `get_state`). Port berisi objek non-`WasmRuntime` hanya bila
        pemiliknya mensubstitusi backend — engine lokal adalah jalur default.
        """
        args = json.dumps(
            {
                "config": {
                    "execution_mode": self._config.execution_mode,
                    "max_concurrent_tasks": self._config.max_concurrent_tasks,
                },
                "agents": [
                    {
                        "id": a.id,
                        "name": a.name,
                        "role": a.role,
                        "system_prompt": a.system_prompt,
                        "max_steps": a.max_steps,
                    }
                    for a in self._agents
                ],
                "task": task,
                "session_id": session_id,
            }
        )
        try:
            self._runtime.call_checked("init", args)
            result_dict = json.loads(self._runtime.call("get_state", args))
        except (WasmRuntimeError, json.JSONDecodeError) as e:
            return TaskResult(
                task_id="",
                agent_id="",
                output=None,
                success=False,
                steps_used=0,
                session_id=session_id or "",
                error=str(e),
            )
        return TaskResult(
            task_id=result_dict.get("task_id", ""),
            agent_id=result_dict.get("agent_id", ""),
            output=result_dict.get("output"),
            success=result_dict.get("success", False),
            steps_used=result_dict.get("steps_used", 0),
            session_id=result_dict.get("session_id", ""),
            error=result_dict.get("error"),
            error_kind=result_dict.get("error_kind"),
            duration_ms=result_dict.get("duration_ms", 0),
        )

    def dispatch(self, task: str, session_id: Optional[str] = None) -> TaskResult:
        """Dispatch a single task to the best-suited agent.

        Runtime di-reuse per sesi (sesi eksplisit dicatat sebelum loop; sesi
        anonim dicatat setelah id aktual diketahui dari engine).

        Args:
            task: The task prompt or description.
            session_id: Optional session ID.

        Returns:
            Task execution result.
        """
        if not isinstance(self._runtime, WasmRuntime):
            return self._dispatch_via_external_backend(task, session_id)
        if session_id is not None:
            runtime = self._acquire_session_runtime(session_id)
        else:
            runtime = self._make_runtime()
        return self._execute_on_runtime(task, session_id, runtime)

    def _worker_count_for(self, task_count: int) -> int:
        """Tabel worker E2.3: sequential=1; concurrent/parallel=min(limit, n);
        auto=1 bila satu task else min(limit, n)."""
        if task_count <= 1:
            return 1
        mode = self._config.execution_mode
        if mode == "sequential":
            return 1
        limit = max(1, self._config.max_concurrent_tasks)
        return min(limit, task_count)

    def dispatch_many(
        self, tasks: list[str], session_ids: Optional[list[str]] = None
    ) -> list[TaskResult]:
        """Dispatch multiple tasks.

        Setiap task dieksekusi pada runtime BARU hasil factory (isolasi Store
        per worker); hasil TERURUT sesuai urutan input. Bila snapshot budget
        sudah exhausted, semua task ditolak sebelum spawn dengan result gagal
        `permanent`.

        Args:
            tasks: List of task prompts.
            session_ids: Optional session IDs, dipetakan per indeks input.

        Returns:
            List of task results in input order.

        Raises:
            ValueError: If `session_ids` length differs from `tasks`.
        """
        task_list = list(tasks)
        if session_ids is None:
            sid_list: list[Optional[str]] = [None] * len(task_list)
        else:
            sid_list = list(session_ids)
            if len(sid_list) != len(task_list):
                raise ValueError(
                    "dispatch_many: session_ids length must match tasks length"
                )

        snapshot = self.get_budget()
        step_exhausted: bool = snapshot["is_step_budget_exhausted"]
        task_exhausted: bool = snapshot["is_task_budget_exhausted"]
        if step_exhausted or task_exhausted:
            if step_exhausted:
                reason = (
                    f"step budget exhausted ({snapshot['consumed_steps']} >= "
                    f"{self._config.max_total_steps})"
                )
            else:
                reason = (
                    f"task budget exhausted ({snapshot['dispatched_tasks']} >= "
                    f"{self._config.max_total_tasks})"
                )
            profile = self._active_profile()
            return [
                TaskResult(
                    task_id="",
                    agent_id=profile.id if profile is not None else "",
                    output=None,
                    success=False,
                    steps_used=0,
                    session_id=sid or "",
                    error=f"orchestrator: {reason}",
                    error_kind="permanent",
                )
                for sid in sid_list
            ]

        max_workers = self._worker_count_for(len(task_list))
        with ThreadPoolExecutor(max_workers=max_workers) as pool:
            futures = [
                pool.submit(self._execute_isolated, task, sid)
                for task, sid in zip(task_list, sid_list)
            ]
            return [future.result() for future in futures]

    def _execute_isolated(
        self, task: str, session_id: Optional[str]
    ) -> TaskResult:
        """Satu task pada runtime segar (dipanggil di thread worker)."""
        return self._execute_on_runtime(task, session_id, self._make_runtime())

    def pipeline(self, tasks: list[str]) -> PipelineResult:
        """Execute tasks sequentially, chaining outputs.

        Args:
            tasks: List of task prompts.

        Returns:
            Pipeline execution result.
        """
        results: list[TaskResult] = []
        previous_output = ""

        for task in tasks:
            input_text = (
                f"Previous output:\n{previous_output}\n\nCurrent task:\n{task}"
                if previous_output
                else task
            )

            result = self.dispatch(input_text)
            results.append(result)

            if isinstance(result.output, str):
                previous_output = result.output
            else:
                previous_output = json.dumps(result.output) if result.output else ""

            if not result.success:
                return PipelineResult(
                    results=results,
                    final_output=result.output,
                    total_steps=sum(r.steps_used for r in results),
                    success=False,
                    error=result.error,
                )

        return PipelineResult(
            results=results,
            final_output=results[-1].output if results else None,
            total_steps=sum(r.steps_used for r in results),
            success=True,
        )

    def cancel(self, session_id: Optional[str] = None) -> bool:
        """Cancel running work cooperatively by resetting runner sessions.

        - `session_id` diberikan: reset sesi itu saja (payload string id
          polos, sesuai kontrak runner satu-parameter).
        - `session_id` None: reset semua sesi yang pernah dicatat orchestrator.
        - Idempoten: True bila >= 1 sesi benar-benar terhapus, False bila
          tidak ada; `WasmRuntimeError` dibiarkan menjadi kontribusi False,
          sedangkan error program tetap raise.
        """
        with self._lock:
            if session_id is not None:
                runtime = self._session_runtimes.pop(session_id, None)
                if runtime is None:
                    return False
                targets = [(session_id, runtime)]
            else:
                targets = list(self._session_runtimes.items())
                self._session_runtimes.clear()
        removed_any = False
        for sid, session_runtime in targets:
            try:
                raw = session_runtime.call("reset_session", sid)
            except WasmRuntimeError:
                continue
            if str(raw).strip().lower() == "true":
                removed_any = True
        return removed_any

    def get_budget(self) -> dict[str, Any]:
        """Get orchestrator budget snapshot (faktual dari hasil engine).

        Returns:
            Budget state dictionary keyed by consumed_steps,
            dispatched_tasks, is_step_budget_exhausted, and
            is_task_budget_exhausted.
        """
        with self._lock:
            consumed = self._consumed_steps
            dispatched = self._dispatched_tasks
        step_limit = self._config.max_total_steps
        task_limit = self._config.max_total_tasks
        return {
            "consumed_steps": consumed,
            "dispatched_tasks": dispatched,
            "is_step_budget_exhausted": (
                step_limit is not None and consumed >= step_limit
            ),
            "is_task_budget_exhausted": (
                task_limit is not None and dispatched >= task_limit
            ),
        }

    def list_agents(self) -> list[AgentProfileConfig]:
        """List all registered agents.

        Returns:
            List of agent profiles.
        """
        return list(self._agents)
