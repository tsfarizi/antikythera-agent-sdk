"""Decorator @tool: function-based tool registration sugar (T3).

Sumber kebenaran:
- clients/python/antikythera_agent/server/registry.py  (DEFAULT + normalize
  6-key; title??None, parameters??[], input_schema??DEFAULT)
- Spec T3: decorator hanya concern attach marker + definition/handler/owner
  ke function asli; nol global auto-registry; deepCopy simpan+return.
"""

from __future__ import annotations

import copy
from typing import Any, Callable, Dict, Optional

from .base import DEFAULT_INPUT_SCHEMA

_VALID_OWNERS = frozenset({"server", "client", "mcp"})


def _normalize_definition(definition: Any) -> Dict[str, Any]:
    """Mirror registry._normalize_definition untuk function decorator."""
    if not isinstance(definition, dict):
        raise ValueError("tool definition requires an object")
    name = definition.get("name")
    if not isinstance(name, str) or not name:
        raise ValueError("tool definition requires a name")
    description = definition.get("description")
    if not isinstance(description, str) or not description:
        raise ValueError("tool definition requires a description")
    input_schema = definition.get("input_schema")
    parameters = definition.get("parameters")
    return {
        "name": name,
        "title": copy.deepcopy(definition.get("title")),
        "description": description,
        "parameters": copy.deepcopy(parameters) if parameters is not None else [],
        "input_schema": copy.deepcopy(
            input_schema if input_schema is not None else DEFAULT_INPUT_SCHEMA
        ),
        "output_schema": copy.deepcopy(definition.get("output_schema")),
    }


def tool(
    name: str,
    description: str,
    owner: str = "server",
    input_schema: Optional[Dict[str, Any]] = None,
    title: Optional[str] = None,
    output_schema: Optional[Dict[str, Any]] = None,
) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    """Decorator penanda function sebagai Tool.

    Args:
        name: nama tool (str non-empty, unik per registry).
        description: deskripsi tool (str non-empty).
        owner: destination `server`|`client`|`mcp` (default `server`).
        input_schema: JSON Schema dict atau None (None -> DEFAULT).
        title: judul opsional atau None.
        output_schema: JSON Schema output atau None.

    Returns:
        Decorator yang menempel `__tool_definition__`, `__tool_handler__`,
        `__tool_owner__`, `__antikythera_tool__` ke function dan mengembalikan
        function asli (nol wrapper, nol auto-registry).
    """
    if not isinstance(name, str) or not name:
        raise ValueError("tool: 'name' must be a non-empty string")
    if not isinstance(description, str) or not description:
        raise ValueError("tool: 'description' must be a non-empty string")
    if not isinstance(owner, str) or owner not in _VALID_OWNERS:
        raise ValueError(
            f"tool: 'owner' must be one of {sorted(_VALID_OWNERS)}, got {owner!r}"
        )
    if input_schema is not None and not isinstance(input_schema, dict):
        raise TypeError("tool: 'input_schema' must be a dict or None")
    if output_schema is not None and not isinstance(output_schema, dict):
        raise TypeError("tool: 'output_schema' must be a dict or None")
    if title is not None and not isinstance(title, str):
        raise TypeError("tool: 'title' must be a string or None")

    normalized = _normalize_definition(
        {
            "name": name,
            "title": title,
            "description": description,
            "parameters": [],
            "input_schema": input_schema,
            "output_schema": output_schema,
        }
    )

    def decorator(fn: Callable[..., Any]) -> Callable[..., Any]:
        if not callable(fn):
            raise TypeError("tool: decorated target must be callable")

        def _tool_definition() -> Dict[str, Any]:
            return copy.deepcopy(normalized)

        def _tool_handler() -> Callable[..., Any]:
            return fn

        fn.__tool_definition__ = _tool_definition  # type: ignore[attr-defined]
        fn.__tool_handler__ = _tool_handler  # type: ignore[attr-defined]
        fn.__tool_owner__ = owner  # type: ignore[attr-defined]
        fn.__antikythera_tool__ = True  # type: ignore[attr-defined]
        return fn

    return decorator
