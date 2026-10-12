"""Host-side wiring for the ``antikythera:agent-sdk/runtime-hooks@1.0.0`` import.

The composite (``dist/antikythera-sdk.wasm``) imports exactly one non-WASI
interface at runtime: ``runtime-hooks``. The host supplies the three decision
functions through the wasmtime component linker; ``WasmRuntime`` invokes
``add_to_linker`` during instantiation.

Provider contract: ``set_provider`` registers an object exposing any subset of
``prepare_turn`` / ``decide_action`` / ``handle_tool_result``, each
``(str, str) -> str`` with the WIT-fixed argument order. Absence of a provider
(or of a method on the provider) is never a failure — the hook returns the
passthrough signal ``{"passthrough": true}`` so the SDK keeps its default
decision. ``set_provider(None)`` clears the registry.
"""

from __future__ import annotations

import json
from typing import Any, Callable, Optional

#: WIT instance name of the composite's single non-WASI import.
RUNTIME_HOOKS_INTERFACE = "antikythera:agent-sdk/runtime-hooks@1.0.0"

#: Passthrough signal: exactly the single-key object defined by the WIT doc.
_PASSTHROUGH_JSON = json.dumps({"passthrough": True})

_provider: Optional[Any] = None


def set_provider(provider: Optional[Any]) -> None:
    """Register the hook decision provider, or clear it by passing ``None``."""
    global _provider
    _provider = provider


def get_provider() -> Optional[Any]:
    """Return the registered provider, or ``None`` when none is configured."""
    return _provider


def _decision(method_name: str, first: str, second: str) -> str:
    """Dispatch a hook to the provider method, defaulting to passthrough."""
    provider = get_provider()
    method = getattr(provider, method_name, None)
    if not callable(method):
        return _PASSTHROUGH_JSON
    return method(first, second)


def prepare_turn(request_json: str, session_state_json: str) -> str:
    """WIT ``prepare-turn(request-json, session-state-json)`` decision."""
    return _decision("prepare_turn", request_json, session_state_json)


def decide_action(session_state_json: str, llm_response_json: str) -> str:
    """WIT ``decide-action(session-state-json, llm-response-json)`` decision."""
    return _decision("decide_action", session_state_json, llm_response_json)


def handle_tool_result(session_state_json: str, tool_result_json: str) -> str:
    """WIT ``handle-tool-result(session-state-json, tool-result-json)`` decision."""
    return _decision("handle_tool_result", session_state_json, tool_result_json)


def add_to_linker(linker: Any, provider_getter: Optional[Callable[[], Any]] = None) -> None:
    """Wire the runtime-hooks import into a wasmtime component linker.

    The wasmtime component ABI lifts the WIT ``result<string, string>`` as a
    tagged variant: the host function must return ``Variant("ok", payload)``
    for the Ok path. Import of ``wasmtime.component`` stays local so this
    module remains importable without the optional wasmtime dependency.
    """
    import wasmtime.component

    hook_impls: dict[str, tuple[str, Callable[[str, str], str]]] = {
        "prepare-turn": ("prepare_turn", prepare_turn),
        "decide-action": ("decide_action", decide_action),
        "handle-tool-result": ("handle_tool_result", handle_tool_result),
    }

    def wrap(decision_fn: Callable[[str, str], str]) -> Callable[..., Any]:
        def impl(_store: Any, first: str, second: str) -> Any:
            try:
                if provider_getter is None:
                    result = decision_fn(first, second)
                else:
                    method = getattr(provider_getter(), decision_fn.__name__, None)
                    result = method(first, second) if callable(method) else _PASSTHROUGH_JSON
                if not isinstance(result, str):
                    result = json.dumps(result)
                return wasmtime.component.Variant("ok", result)
            except Exception as exc:
                return wasmtime.component.Variant("err", str(exc))

        return impl

    with linker.root() as root:
        with root.add_instance(RUNTIME_HOOKS_INTERFACE) as hooks:
            for export_name, (_, decision_fn) in hook_impls.items():
                hooks.add_func(export_name, wrap(decision_fn))
