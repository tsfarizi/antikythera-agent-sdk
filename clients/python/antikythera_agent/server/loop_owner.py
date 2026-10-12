"""K1 tool loop owner (unit U23): loop agent host-side untuk mode core@server.

Sumber kebenaran perilaku: `antikythera-server-runtime/src/loop_owner.rs`
(di-mirror 1:1; lihat juga `documentation/WIRE_PROTOCOL.md` §6 mapping ke
runner contract dan `wit/antikythera.wit` interface `runner`).

Alur K1 (`auto_execute_tools=false`, host yang mengeksekusi tool):
`init(config)` → `prepare_user_turn` → LLM via `resolve_provider(...).call(...)`
→ `commit_llm_response` → `drain_events` → pada `call_tool`:
(a) builtin in-band: drain SUDAH berisi `tool_result` untuk tool tersebut —
    komposit mengeksekusi builtin di dalam commit (llm_stream.rs) sehingga
    TIDAK perlu host execution (tool tidak dieksekusi dua kali);
(b) selain itu routing → `process_tool_result_for_session` → ulang sampai
    `final` / `max_steps` / `retry`.

Routing tool (mirror `routing.rs::ToolRouter::execute`): owner `server` →
handler lokal via `registry.handler_of`; owner `client` →
`tool-execution-request` di `ControlChannel` + `await_postback`; owner `mcp`
→ error eksplisit (transport MCP Python belum ada). Tool tanpa owner di union
registry → denial `permission:`. `gate.check_tool(destination, name)` dipanggil
SEBELUM eksekusi lokal/remote; denial `PermissionDeniedError` berprefix
`permission:` meluas sebagai error loop (gate.py: "loop_owner membiarkannya
meluas sebagai error loop").

Hook runtime (`runtime_hooks_enabled=true`): provider runtime-hooks dipasang ke
`antikythera_agent.host` yang meneruskan setiap keputusan (`prepare-turn`,
`decide-action`, `handle-tool-result`) ke peer sebagai `hook-request` SSE +
POST-back (mirror `wit.rs::request_hook_decision`); provider dipasang hanya
selama loop dan dipulihkan setelahnya (registry host adalah modul-global).

Amplop runtime:
- `max_steps` adalah batas jumlah ITERASI LLM (bukan langkah tool);
  terlampaui → `ToolLoopError` dengan pesan mirror Rust.
- Error domain loop = `ToolLoopError`; denial gate meluas sebagai
  `PermissionDeniedError` berprefix `permission:`; timeout POST-back meluas
  sebagai `PendingTimeoutError` berprefix `permission:` (fail-closed
  WIRE_PROTOCOL §5 — tanpa silent hang).
- `provider` parameter adalah resolver `(name) -> LlmProvider` (mirror
  `SharedState::resolve_provider`); `None` memakai `resolve_provider` default
  dari `antikythera_agent.server.provider`.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional

from antikythera_agent.server import wire
from antikythera_agent.server.provider import resolve_provider

#: Prefix denial invariant R4 (gate.py / control.py memakai nilai identik).
_PERMISSION_PREFIX = "permission:"

#: Resolver LLM: `(name) -> LlmProvider` (mirror `SharedState::resolve_provider`).
ProviderResolver = Callable[[Optional[str]], Any]


class ToolLoopError(Exception):
    """Error domain loop owner (mirror `Result<_, String>` loop_owner.rs)."""


def _normalize_denial(message: str) -> str:
    """Denial dari POST-back WAJIB berprefix `permission:` (R4): client yang
    mengirim pesan denial tanpa prefix tidak boleh melemahkan invariant."""
    if message.startswith(_PERMISSION_PREFIX):
        return message
    return f"{_PERMISSION_PREFIX} {message}"


def _parse_json(text: str, label: str) -> Any:
    try:
        return json.loads(text)
    except (TypeError, json.JSONDecodeError) as exc:
        raise ToolLoopError(f"tool loop: {label} is not JSON: {exc}") from exc


@dataclass
class ToolLoopConfig:
    """Parameter satu run tool loop (mirror `loop_owner.rs::ToolLoopConfig`).

    `client_id` dan `pending_ttl_secs` adalah seam routing remote (mirror
    `ServerRuntimeConfig.client_id` / `pending_ttl`): identitas peer client
    SSE dan TTL POST-back (WIRE_PROTOCOL §5, default 60s).
    """

    session_id: str = "server-loop"
    max_steps: int = 10
    #: Per-step prompts; step `i` memakai `prompts[i]` bila ada, else prompt terakhir.
    prompts: List[str] = field(default_factory=lambda: ["hello"])
    provider: str = "stub"
    model: str = "stub-model"
    temperature: Optional[float] = None
    max_tokens: Optional[int] = None
    force_json: bool = False
    #: True → definisi union registry di-push via `register-tools` sebelum loop.
    register_union_tools: bool = True
    #: True → runtime-hooks host dikonsultasikan (hook-request ke peer client).
    runtime_hooks_enabled: bool = False
    client_id: str = "antikythera-client"
    pending_ttl_secs: float = 60.0

    def __post_init__(self) -> None:
        if not isinstance(self.session_id, str) or not self.session_id:
            raise ValueError("tool loop: session_id must be a non-empty string")
        if (
            not isinstance(self.max_steps, int)
            or isinstance(self.max_steps, bool)
            or self.max_steps < 0
        ):
            raise ValueError("tool loop: max_steps must be a non-negative integer")
        if not isinstance(self.prompts, list) or not all(isinstance(p, str) for p in self.prompts):
            raise ValueError("tool loop: prompts must be a list of strings")
        if not isinstance(self.provider, str) or not self.provider:
            raise ValueError("tool loop: provider must be a non-empty string")
        if not isinstance(self.model, str):
            raise ValueError("tool loop: model must be a string")
        for name in ("force_json", "register_union_tools", "runtime_hooks_enabled"):
            if not isinstance(getattr(self, name), bool):
                raise ValueError(f"tool loop: {name} must be a bool")
        if self.temperature is not None and (
            not isinstance(self.temperature, (int, float)) or isinstance(self.temperature, bool)
        ):
            raise ValueError("tool loop: temperature must be a number or None")
        if self.max_tokens is not None and (
            not isinstance(self.max_tokens, int)
            or isinstance(self.max_tokens, bool)
            or self.max_tokens < 0
        ):
            raise ValueError("tool loop: max_tokens must be a non-negative integer or None")
        if not isinstance(self.client_id, str) or not self.client_id:
            raise ValueError("tool loop: client_id must be a non-empty string")
        if (
            not isinstance(self.pending_ttl_secs, (int, float))
            or isinstance(self.pending_ttl_secs, bool)
            or self.pending_ttl_secs <= 0
        ):
            raise ValueError("tool loop: pending_ttl_secs must be a positive number")


@dataclass
class LoopOutcome:
    """Hasil loop yang mencapai `final` (mirror `loop_owner.rs::LoopOutcome`)."""

    session_id: str
    steps: int
    action: str
    content: Optional[str]
    commit_json: Any


def _execute_local(registry: Any, tool: str, tool_input: Any, step_id: int) -> Dict[str, Any]:
    """Eksekusi tool owner `server`: handler lokal; kegagalan handler adalah
    hasil `success=false`, bukan error (mirror `routing.rs::execute_local`).

    Kontrak handler: `handler(arguments) -> output` (JSON-serializable) untuk
    sukses; melempar exception untuk kegagalan tool.
    """
    handler = registry.handler_of(tool)
    if handler is None:
        raise ToolLoopError(f"tool '{tool}' has no local handler")
    try:
        output = handler(tool_input)
    except Exception as exc:
        return {
            "tool-name": tool,
            "success": False,
            "output-json": "{}",
            "error-message": str(exc),
            "step-id": step_id,
        }
    try:
        output_json = json.dumps(output)
    except (TypeError, ValueError) as exc:
        return {
            "tool-name": tool,
            "success": False,
            "output-json": "{}",
            "error-message": f"handler output is not JSON-serializable: {exc}",
            "step-id": step_id,
        }
    return {
        "tool-name": tool,
        "success": True,
        "output-json": output_json,
        "error-message": None,
        "step-id": step_id,
    }


def _execute_remote(
    control: Any,
    config: ToolLoopConfig,
    tool: str,
    tool_input: Any,
    session_id: str,
    step_id: int,
) -> Dict[str, Any]:
    """Eksekusi tool owner `client`: `tool-execution-request` SSE + POST-back
    (mirror `routing.rs::execute_remote`). Fail-closed: client tidak terhubung
    atau timeout POST-back → error `permission:`.


    `tool_input` None dinormalisasi ke {} agar arguments-json wire tetap object (golden
    `tool_execute_request.arguments-json`).
    """
    if control is None:
        raise ToolLoopError(f"permission: tool '{tool}' requires a connected client")
    correlation_id = control.create_correlation(config.client_id, config.pending_ttl_secs)
    if not control.is_client_connected(config.client_id):
        control.cancel_pending(correlation_id)
        raise ToolLoopError(f"permission: tool '{tool}' requires a connected client")
    envelope = {
        "type": "tool-execution-request",
        "correlation_id": correlation_id,
        "session_id": session_id,
        "client_id": config.client_id,
        "payload": wire.build_tool_call_event(
            {
                "tool_name": tool,
                "arguments_json": json.dumps(tool_input if tool_input is not None else {}),
                "session_id": session_id,
                "step_id": step_id,
            }
        ),
    }
    control.push(config.client_id, envelope)
    body = control.await_postback(correlation_id, config.pending_ttl_secs)
    if not body.get("ok"):
        error = body.get("error") or f"permission: tool '{tool}' rejected by client"
        raise ToolLoopError(_normalize_denial(error))
    result = body.get("payload")
    if not isinstance(result, dict):
        raise ToolLoopError(f"tool '{tool}': client returned invalid tool-execution-result")
    return wire.parse_tool_execution_result(result)


def _execute_tool(
    registry: Any,
    gate: Any,
    control: Any,
    config: ToolLoopConfig,
    tool: str,
    tool_input: Any,
    session_id: str,
    step_id: int,
) -> Dict[str, Any]:
    """Routing satu tool non-in-band (mirror `routing.rs::ToolRouter::execute`):
    resolusi owner → gate check per destination → dispatch. Mengembalikan
    shape wire `tool-execution-result` (kebab-case).


    Gate check SEBELUM dispatch untuk semua destination (default-deny R4).

    Owner mcp: transport MCP belum ada di paket Python; gagal eksplisit, bukan degradasi
    senyap (amplop unit: routing mcp = program error).
    """
    owner = registry.owner_of(tool)
    if owner is None:
        raise ToolLoopError(f"permission: tool '{tool}' not in allowlist")
    gate.check_tool(owner, tool)
    if owner in (wire.WIRE["OWNER_SERVER"], wire.WIRE["OWNER_MCP"]):
        return _execute_local(registry, tool, tool_input, step_id)
    if owner == wire.WIRE["OWNER_CLIENT"]:
        return _execute_remote(control, config, tool, tool_input, session_id, step_id)
    raise ToolLoopError(f"tool '{tool}' has no MCP transport in the Python server runtime")


class _RemoteHookProvider:
    """Provider `antikythera_agent.host` yang meneruskan keputusan hook ke
    peer lewat `ControlChannel` (`hook-request` SSE + POST-back), mirror
    `wit.rs::request_hook_decision`. Dipasang hanya saat
    `runtime_hooks_enabled=true`."""

    def __init__(
        self,
        gate: Any,
        control: Any,
        client_id: str,
        session_id: str,
        pending_ttl_secs: float,
    ) -> None:
        self._gate = gate
        self._control = control
        self._client_id = client_id
        self._session_id = session_id
        self._pending_ttl_secs = pending_ttl_secs

    def prepare_turn(self, request_json: str, session_state_json: str) -> str:
        """WIT `prepare-turn(request-json, session-state-json)`."""
        return self._request_decision(
            wire.WIRE["HOOK_PREPARE_TURN"], session_state_json, request_json
        )

    def decide_action(self, session_state_json: str, llm_response_json: str) -> str:
        """WIT `decide-action(session-state-json, llm-response-json)`."""
        return self._request_decision(
            wire.WIRE["HOOK_DECIDE_ACTION"], session_state_json, llm_response_json
        )

    def handle_tool_result(self, session_state_json: str, tool_result_json: str) -> str:
        """WIT `handle-tool-result(session-state-json, tool-result-json)`."""
        return self._request_decision(
            wire.WIRE["HOOK_HANDLE_TOOL_RESULT"], session_state_json, tool_result_json
        )

    def _request_decision(self, hook: str, session_state_json: str, input_json: str) -> str:
        """Satu keputusan hook ke peer: gate → korelasi → push → POST-back.
        Decision adalah string JSON object (kontrak WIT runtime-hooks)."""
        self._gate.check_hook(hook)
        if self._control is None:
            raise ToolLoopError(f"permission: hook '{hook}' requires a connected client")
        correlation_id = self._control.create_correlation(self._client_id, self._pending_ttl_secs)
        if not self._control.is_client_connected(self._client_id):
            self._control.cancel_pending(correlation_id)
            raise ToolLoopError(f"permission: hook '{hook}' requires a connected client")
        envelope = {
            "type": "hook-request",
            "correlation_id": correlation_id,
            "session_id": self._session_id,
            "client_id": self._client_id,
            "payload": {
                "hook": hook,
                "session_state_json": session_state_json,
                "input_json": input_json,
            },
        }
        self._control.push(self._client_id, envelope)
        body = self._control.await_postback(correlation_id, self._pending_ttl_secs)
        if not body.get("ok"):
            error = body.get("error") or f"permission: hook '{hook}' rejected by client"
            raise ToolLoopError(_normalize_denial(error))
        payload = body.get("payload")
        if isinstance(payload, str):
            return payload
        if payload is None:
            raise ToolLoopError(f"permission: hook '{hook}' response payload is not a decision")
        return json.dumps(payload)


def run_tool_loop(
    runtime: Any,
    registry: Any,
    gate: Any,
    provider: Optional[ProviderResolver],
    control: Any,
    config: ToolLoopConfig,
) -> LoopOutcome:
    """Jalankan tool loop (blocking, thread core; mirror `loop_owner.rs`).

    Args:
        runtime: `WasmRuntime` ter-instantiasi (komposit SDK + toolrunner).
        registry: `UnionRegistry` (definisi union + owner/handler routing).
        gate: `PolicyGate` default-deny; denial → `PermissionDeniedError`.
        provider: resolver `(name) -> LlmProvider`; `None` → `resolve_provider`.
        control: `ControlChannel` (remote tool `client` + hook-request).
        config: parameter loop (`ToolLoopConfig`).

    Returns:
        `LoopOutcome` saat mencapai `final`.

    Raises:
        ToolLoopError: max_steps terlampaui / LLM gagal / action tak dikenal /
            retry / kegagalan routing atau POST-back denial.
        PermissionDeniedError: denial gate (`permission:` prefix).
        PendingTimeoutError: POST-back timeout (`permission:` prefix).
        WasmRuntimeError: kegagalan panggilan runner.


    Builtin in-band: komposit sudah mengeksekusi tool di dalam commit dan mengemisi
    tool_result ke drain — tanpa host execution, tanpa double execution (invariant U23).
    """
    if provider is not None and not callable(provider):
        raise TypeError("tool loop: provider must be a callable resolver (name) -> LlmProvider")
    resolver = resolve_provider if provider is None else provider

    config_json = json.dumps(
        {
            "session_id": config.session_id,
            "max_steps": config.max_steps,
            "auto_execute_tools": False,
            "runtime_hooks_enabled": config.runtime_hooks_enabled,
        }
    )
    session_id = runtime.call("init", config_json)

    if config.register_union_tools:
        union_json = json.dumps(registry.definitions())
        runtime.call("register_tools", union_json)

    hook_provider = None
    previous_host_provider = getattr(runtime, "hooks_provider", None)
    if config.runtime_hooks_enabled:
        hook_provider = _RemoteHookProvider(
            gate, control, config.client_id, session_id, config.pending_ttl_secs
        )
        runtime.hooks_provider = hook_provider
    try:
        step = 0
        while True:
            if step >= config.max_steps:
                raise ToolLoopError(
                    f"tool loop: max_steps ({config.max_steps}) exceeded without final action"
                )
            prompt = (
                config.prompts[step]
                if step < len(config.prompts)
                else (config.prompts[-1] if config.prompts else "")
            )
            request_json = json.dumps(
                {
                    "prompt": prompt,
                    "session_id": session_id,
                    "correlation_id": f"loop-{step}",
                }
            )
            prepared_json = runtime.call("prepare_user_turn", request_json)
            prepared = _parse_json(prepared_json, "prepared turn")
            messages_json = prepared.get("messages_json") or "[]"

            llm_request = wire.build_llm_request(
                {
                    "provider": config.provider,
                    "model": config.model,
                    "session_id": session_id,
                    "messages_json": messages_json,
                    "force_json": config.force_json,
                    "temperature": config.temperature,
                    "max_tokens": config.max_tokens,
                }
            )
            gate.check_llm(session_id)
            try:
                llm_response = resolver(config.provider).call(llm_request)
            except Exception as exc:
                raise ToolLoopError(f"tool loop: llm call failed: {exc}") from exc
            if not isinstance(llm_response, dict) or "content" not in llm_response:
                raise ToolLoopError("tool loop: llm call failed: provider returned no content")

            commit_json = _parse_json(
                runtime.call(
                    "commit_llm_response",
                    json.dumps([prepared_json, llm_response.get("content", "")]),
                ),
                "commit result",
            )
            action = commit_json.get("action") or ""
            content = commit_json.get("content")

            if action == "final":
                return LoopOutcome(
                    session_id=session_id,
                    steps=step + 1,
                    action=action,
                    content=content,
                    commit_json=commit_json,
                )

            if action == "call_tool":
                tool = commit_json.get("tool_name") or ""
                tool_input = commit_json.get("tool_input")
                step_id = commit_json.get("step") or 0

                events = _parse_json(runtime.call("drain_events", session_id), "drained events")
                in_band = isinstance(events, list) and any(
                    isinstance(event, dict)
                    and event.get("kind") == "tool_result"
                    and (event.get("payload") or {}).get("tool") == tool
                    for event in events
                )

                if not in_band:
                    wire_result = _execute_tool(
                        registry,
                        gate,
                        control,
                        config,
                        tool,
                        tool_input,
                        session_id,
                        step_id,
                    )
                    correlation_id = commit_json.get("correlation_id")
                    runner_input = wire.wire_to_runner_tool_result(wire_result, correlation_id)
                    runtime.call(
                        "process_tool_result_for_session",
                        json.dumps([session_id, json.dumps(runner_input)]),
                    )
            elif action == "retry":
                raise ToolLoopError(f"tool loop: runner requested retry: {content or ''}")
            else:
                raise ToolLoopError(f"tool loop: unknown action '{action}'")
            step += 1
    finally:
        if hook_provider is not None:
            runtime.hooks_provider = previous_host_provider
