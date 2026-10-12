"""Local tool loop engine (unit UP2): agent host-side loop tanpa server wrapper.

Sumber kebenaran perilaku: debug/U1-design-notes.md E1 + interface `runner`
(`wit/antikythera.wit`); loop body di-mirror dari
`antikythera_agent/server/loop_owner.py` dengan routing dibatasi owner
`server` — tanpa control channel, hook remote, maupun transport (S2).

Alur (`auto_execute_tools=false`, host yang mengeksekusi tool):
`init(config)` -> [`register_tools`] -> per iterasi: `prepare_user_turn` ->
LLM via resolver -> `commit_llm_response` -> `drain_events` -> pada
`call_tool`: builtin in-band (drain sudah memuat `tool_result`) skip host
execution, selain itu gate check -> handler lokal ->
`process_tool_result_for_session` -> ulang sampai `final` / `max_steps` /
`retry`.

Amplop runtime:
- `max_steps` adalah batas iterasi LLM (bukan langkah tool); terlampaui ->
  `ToolLoopError` — loop tidak me-retry kegagalan internal.
- Routing lokal hanya owner `server`; `client`/`mcp`/tak-dikenal fail-closed
  sebagai error berprefix `permission:`. Denial gate meluas apa adanya
  (`PermissionDeniedError` berprefix `permission:`).
- `provider_resolver=None` -> `server.provider.resolve_provider`; bila
  provider mengekspos atribut `timeout`, ia diset (detik) dari
  `config.timeout` ms sebelum panggilan.
- Batas volume: `max_steps < 10k`, drain < 1k event/iterasi — sequential
  LLM-bound; di luar amplop itu gagal eksplisit, bukan degradasi senyap.
"""

from __future__ import annotations

import copy
import json
from dataclasses import dataclass
from typing import Any, Callable, Dict, List, Optional

from antikythera_agent.server import wire
from antikythera_agent.server.provider import resolve_provider

#: Prefix denial invariant R4 (identik dengan loop_owner / gate).
_PERMISSION_PREFIX = "permission:"

#: Resolver LLM: `(name) -> LlmProvider` (mirror `SharedState::resolve_provider`).
ProviderResolver = Callable[[Optional[str]], Any]


class ToolLoopError(Exception):
    """Error domain local loop (mirror `loop_owner.ToolLoopError`)."""


def _parse_json(text: str, label: str) -> Any:
    try:
        return json.loads(text)
    except (TypeError, json.JSONDecodeError) as exc:
        raise ToolLoopError(f"local loop: {label} is not JSON: {exc}") from exc


@dataclass
class LocalLoopConfig:
    """Parameter satu run local loop (turunan `ToolLoopConfig` tanpa seam
    remote: tanpa `client_id`, `pending_ttl_secs`, flag register/hooks).

    `session_id` kosong berarti runner yang membuat session baru; nilai balik
    `init` (string polos) adalah sumber kebenaran session.
    """

    session_id: str
    max_steps: int
    provider: str
    model: str
    system_prompt: Optional[str]
    timeout: int
    force_json: bool = False
    temperature: Optional[float] = None
    max_tokens: Optional[int] = None
    prompts: Optional[List[str]] = None
    runtime_hooks_enabled: bool = False

    def __post_init__(self) -> None:
        if not isinstance(self.session_id, str):
            raise ValueError("local loop: session_id must be a string")
        if (
            not isinstance(self.max_steps, int)
            or isinstance(self.max_steps, bool)
            or self.max_steps < 0
        ):
            raise ValueError("local loop: max_steps must be a non-negative integer")
        if not isinstance(self.provider, str) or not self.provider:
            raise ValueError("local loop: provider must be a non-empty string")
        if not isinstance(self.model, str):
            raise ValueError("local loop: model must be a string")
        if not isinstance(self.timeout, int) or isinstance(self.timeout, bool) or self.timeout < 0:
            raise ValueError("local loop: timeout must be a non-negative integer")
        if not isinstance(self.force_json, bool):
            raise ValueError("local loop: force_json must be a bool")
        if not isinstance(self.runtime_hooks_enabled, bool):
            raise ValueError("local loop: runtime_hooks_enabled must be a bool")
        if self.temperature is not None and (
            not isinstance(self.temperature, (int, float)) or isinstance(self.temperature, bool)
        ):
            raise ValueError("local loop: temperature must be a number or None")
        if self.max_tokens is not None and (
            not isinstance(self.max_tokens, int)
            or isinstance(self.max_tokens, bool)
            or self.max_tokens < 0
        ):
            raise ValueError("local loop: max_tokens must be a non-negative integer or None")
        if self.prompts is not None and (
            not isinstance(self.prompts, list) or not all(isinstance(p, str) for p in self.prompts)
        ):
            raise ValueError("local loop: prompts must be a list of strings or None")


@dataclass
class LoopOutcome:
    """Hasil loop yang mencapai `final` (mirror `loop_owner.LoopOutcome` +
    CommitResult; `drained_events` disertakan untuk observability)."""

    session_id: str
    steps: int
    action: str
    content: Optional[str]
    commit_json: Any
    drained_events: Optional[List[Any]] = None


def _execute_local(registry: Any, tool: str, tool_input: Any, step_id: int) -> Dict[str, Any]:
    """Eksekusi tool owner `server`: handler lokal; kegagalan handler adalah
    hasil `success=false`, bukan error (mirror `routing.rs::execute_local` via
    `loop_owner._execute_local`)."""
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


def _route_local_tool(
    registry: Any,
    gate: Any,
    tool: str,
    tool_input: Any,
    step_id: int,
) -> Dict[str, Any]:
    """Routing satu tool non-in-band, dibatasi owner `server` (mirror
    `loop_owner._execute_tool` tanpa branch control/mcp): resolusi owner ->
    gate check SEBELUM eksekusi (default-deny R4) -> dispatch lokal.


    Gate check sebelum dispatch; denial (`permission:` prefix) meluas apa adanya sebagai error
    loop — bukan degradasi senyap.
    """
    owner = registry.owner_of(tool) if registry is not None else None
    if owner is None:
        raise ToolLoopError(f"{_PERMISSION_PREFIX} tool '{tool}' not in allowlist")
    gate.check_tool(owner, tool)
    if owner != wire.WIRE["OWNER_SERVER"]:
        raise ToolLoopError(
            f"{_PERMISSION_PREFIX} tool '{tool}' requires '{owner}' routing "
            "(not available in the local loop)"
        )
    return _execute_local(registry, tool, tool_input, step_id)


def run_local_loop(
    runtime: Any,
    provider_resolver: Optional[ProviderResolver],
    registry: Any,
    gate: Any,
    config: LocalLoopConfig,
    *,
    cancel_event: Any = None,
    on_step: Optional[Callable[[int], None]] = None,
) -> LoopOutcome:
    """Jalankan local tool loop (blocking; mirror `run_tool_loop` Wave 1).

    Args:
        runtime: `WasmRuntime` ter-instantiasi (komposit SDK + toolrunner).
        provider_resolver: resolver `(name) -> LlmProvider`; `None` ->
            `server.provider.resolve_provider`.
        registry: `UnionRegistry` atau `None` (tanpa tool lokal — semua
            permintaan tool fail-closed `permission:`).
        gate: `PolicyGate` default-deny; wajib bersama registry non-None.
        config: parameter loop (`LocalLoopConfig`).

    Returns:
        `LoopOutcome` saat mencapai `final`.

    Raises:
        ToolLoopError: max_steps terlampaui / LLM gagal / action tak dikenal /
            retry / denial routing.
        PermissionDeniedError: denial gate (`permission:` prefix).
        WasmRuntimeError: kegagalan panggilan runner.


    Init mengembalikan string session-id polos (mod.rs:203 — dibuat runner bila payload tidak
    menyertakan session_id).

    Kontrak timeout E1.3: ms config -> detik atribut provider.

    Builtin in-band: komposit mengeksekusi builtin di dalam commit dan mengemisi tool_result
    ke drain — host TIDAK mengeksekusi lagi (tanpa double execution, invariant U23).
    """
    if provider_resolver is not None and not callable(provider_resolver):
        raise TypeError("local loop: provider must be a callable resolver (name) -> LlmProvider")
    resolver = resolve_provider if provider_resolver is None else provider_resolver

    config_payload: Dict[str, Any] = {
        "max_steps": config.max_steps,
        "auto_execute_tools": False,
        "runtime_hooks_enabled": config.runtime_hooks_enabled,
    }
    if config.session_id:
        config_payload["session_id"] = config.session_id
    session_id = runtime.call("init", json.dumps(config_payload))
    if not isinstance(session_id, str) or not session_id:
        raise ToolLoopError("local loop: init did not return a session id")

    if registry is not None:
        runtime.call("register_tools", json.dumps(registry.definitions()))

    prompts = config.prompts or []
    step = 0
    while True:
        if cancel_event is not None and cancel_event.is_set():
            raise ToolLoopError("local loop: cancelled")
        if step >= config.max_steps:
            raise ToolLoopError(
                f"local loop: max_steps ({config.max_steps}) exceeded without final action"
            )
        prompt = prompts[step] if step < len(prompts) else (prompts[-1] if prompts else "")
        request_json = json.dumps(
            {
                "prompt": prompt,
                "session_id": session_id,
                "system_prompt": config.system_prompt,
                "force_json": config.force_json,
                "correlation_id": f"local-loop-{step}",
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
        try:
            if on_step is not None:
                on_step(step + 1)
            provider = resolver(config.provider)
            from antikythera_agent.server.provider import OllamaProvider, OpenAiProvider

            if isinstance(provider, (OllamaProvider, OpenAiProvider)):
                provider = copy.copy(provider)
            if hasattr(provider, "timeout"):
                provider.timeout = config.timeout / 1000
            raw_response = provider.call(llm_request)
            llm_response = wire.parse_llm_response(raw_response)
        except Exception as exc:
            raise ToolLoopError(f"local loop: llm call failed: {exc}") from exc

        if cancel_event is not None and cancel_event.is_set():
            raise ToolLoopError("local loop: cancelled")
        commit_json = _parse_json(
            runtime.call(
                "commit_llm_response",
                json.dumps([prepared_json, llm_response["content"]]),
            ),
            "commit result",
        )
        action = commit_json.get("action") or ""
        content = commit_json.get("content")

        events = _parse_json(runtime.call("drain_events", session_id), "drained events")

        if action == "final":
            return LoopOutcome(
                session_id=session_id,
                steps=step + 1,
                action=action,
                content=content,
                commit_json=commit_json,
                drained_events=events,
            )

        if action == "call_tool":
            tool = commit_json.get("tool_name") or ""
            tool_input = commit_json.get("tool_input")
            step_id = commit_json.get("step") or 0

            in_band = isinstance(events, list) and any(
                isinstance(event, dict)
                and event.get("kind") == "tool_result"
                and (event.get("payload") or {}).get("tool") == tool
                for event in events
            )

            if not in_band:
                if cancel_event is not None and cancel_event.is_set():
                    raise ToolLoopError("local loop: cancelled")
                wire_result = _route_local_tool(registry, gate, tool, tool_input, step_id)
                correlation_id = commit_json.get("correlation_id")
                runner_input = wire.wire_to_runner_tool_result(wire_result, correlation_id)
                runtime.call(
                    "process_tool_result_for_session",
                    json.dumps([session_id, json.dumps(runner_input)]),
                )
        elif action == "retry":
            raise ToolLoopError(f"local loop: runner requested retry: {content or ''}")
        else:
            raise ToolLoopError(f"local loop: unknown action '{action}'")
        step += 1
