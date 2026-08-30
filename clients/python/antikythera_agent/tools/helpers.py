"""Helpers: register_tool / is_tool / extract_tool_entry (T3).

Sumber kebenaran:
- clients/python/antikythera_agent/server/registry.py  (UnionRegistry shape
  golden 6-key; pesan collision kanonik existing-first; sort deterministik)
- clients/python/antikythera_agent/server/gate.py       (allow_tool
  signature `allow_tool(destination, name)`; default-deny; prefix permission:)
- Spec T3: helper hanya concern translasi sugar -> registry+gate; validasi
  keys ⊆ golden; register != allow strict; deepCopy ditangani normalize.
"""

from __future__ import annotations

import copy
from typing import Any, Dict

_GOLDEN_KEYS = frozenset(
    {"name", "title", "description", "parameters", "input_schema", "output_schema"}
)
_VALID_OWNERS = frozenset({"server", "client", "mcp"})


def is_tool(obj: Any) -> bool:
    """True bila obj adalah Tool sugar (BaseTool subclass atau @tool function)."""
    if getattr(obj, "__antikythera_tool__", None) is not True:
        return False
    defn = getattr(obj, "__tool_definition__", None)
    handler = getattr(obj, "__tool_handler__", None)
    return callable(defn) and callable(handler)


def extract_tool_entry(obj: Any) -> Dict[str, Any]:
    """Ekstrak `{definition, handler, owner}` dari Tool sugar.

    Raises:
        TypeError: bila obj bukan Tool.
        ValueError: bila owner/definiton tidak valid atau mengandung kunci
            di luar golden 6-key.
    """
    if not is_tool(obj):
        raise TypeError(f"object {obj!r} is not a Tool (missing @tool or BaseTool)")
    owner = getattr(obj, "__tool_owner__", None)
    if owner is None:
        owner = getattr(obj, "owner", "server")
    if not isinstance(owner, str):
        raise TypeError(f"tool owner must be a string, got {type(owner).__name__}")
    if owner not in _VALID_OWNERS:
        raise ValueError(
            f"unknown tool owner {owner!r}; expected one of {sorted(_VALID_OWNERS)}"
        )
    defn_callable = getattr(obj, "__tool_definition__", None)
    if not callable(defn_callable):
        raise TypeError("tool is missing __tool_definition__")
    definition = defn_callable()
    if not isinstance(definition, dict):
        raise TypeError("tool definition must be a dict")
    extra = set(definition.keys()) - _GOLDEN_KEYS
    if extra:
        raise ValueError(
            f"tool definition contains invalid keys {sorted(extra)}; "
            f"allowed {sorted(_GOLDEN_KEYS)}"
        )
    handler_callable = getattr(obj, "__tool_handler__", None)
    if not callable(handler_callable):
        raise TypeError("tool is missing __tool_handler__")
    handler = handler_callable()
    if handler is not None and not callable(handler):
        raise TypeError("tool handler must be callable or None")
    if owner == "server" and not callable(handler):
        raise TypeError("server tool handler must be callable")
    return {
        "definition": copy.deepcopy(definition),
        "handler": handler,
        "owner": owner,
    }


def register_tool(registry: Any, gate: Any, obj: Any, allow: bool = False) -> None:
    """Daftarkan Tool sugar ke UnionRegistry (+ opsional allowlist gate).

    Proses:
      1) extract via `extract_tool_entry` (TypeError bila bukan Tool),
      2) `registry.register_*` sesuai owner (server/client/mcp) — collision
         message kanonik existing-first dijaga registry; sort ditangani registry,
      3) `gate.allow_tool(owner, name)` HANYA bila `allow=True` (strict
         register != allow), else JANGAN panggil gate,
      4) validasi keys ⊆ golden 6-key (ValueError bila ada kunci ekstra).

    Args:
        registry: `UnionRegistry` instance.
        gate: `PolicyGate` instance (boleh None bila `allow=False`).
        obj: subclass `BaseTool` atau function `@tool`.
        allow: bila True, sekalian allowlist di gate.

    Raises:
        TypeError: bukan Tool atau handler/allow tidak valid.
        ValueError: owner tidak dikenal atau kunci definisi ekstra;
            juga collision `tool registry: name collision ...` dari registry.
    """
    if not isinstance(allow, bool):
        raise TypeError("register_tool: 'allow' must be a bool")
    entry = extract_tool_entry(obj)
    definition: Dict[str, Any] = entry["definition"]
    owner: str = entry["owner"]
    handler: Any = entry["handler"]

    if owner == "server":
        registry.register_server(definition, handler)
    elif owner == "client":
        registry.register_client(definition)
    elif owner == "mcp":
        registry.register_mcp(definition)
    else:
        raise ValueError(f"unknown tool owner {owner!r}")

    if allow:
        if gate is None:
            raise TypeError("register_tool: gate is required when allow=True")
        if not hasattr(gate, "allow_tool") or not callable(getattr(gate, "allow_tool")):
            raise TypeError("register_tool: gate must have allow_tool method")
        gate.allow_tool(owner, definition["name"])
