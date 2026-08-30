"""BaseTool: class-based tool registration sugar (T3, mirror server/registry.py).

Sumber kebenaran:
- clients/python/antikythera_agent/server/registry.py  (normalize + DEFAULT +
  shape golden 6-key; pesan collision kanonik; deepCopy simpan+return;
  sort deterministik registry)
- Spec T2/T3: BaseTool hanya memegang concern definisi + handler binding;
  nol global auto-registry on import; owner disimpan terpisah dari definition.
"""

from __future__ import annotations

import copy
from typing import Any, Callable, Dict, Optional

#: Default schema input tool (mirror server/registry.py DEFAULT_INPUT_SCHEMA).
DEFAULT_INPUT_SCHEMA: Dict[str, Any] = {"type": "object", "properties": {}, "required": []}

#: Owner yang dikontrak (mirror wire.WIRE OWNER_* / gate.DESTINATIONS).
_VALID_OWNERS = frozenset({"server", "client", "mcp"})


def _normalize_definition(definition: Any) -> Dict[str, Any]:
    """Normalisasi definisi ke shape golden 6-key (mirror registry._normalize_definition).

    `name`/`description` wajib non-empty string; `title`/`output_schema`
    default None; `parameters` default []; `input_schema` default DEFAULT.
    Nilai yang diberikan dipertahankan apa adanya (mirror `??` — None diganti
    default, falsy non-None tidak). Semua nilai disalin agar mutasi pemanggil
    tidak bocor ke registry / definition store.
    """
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


class BaseTool:
    """Basis class-based tool (T3 sugar).

    Subclass wajib mengisi `name` (str non-empty) dan `description`
    (str non-empty). Field opsional: `title` (str|None), `input_schema`
    (dict, default object kosong), `output_schema` (dict|None), `owner`
    (str server|client|mcp, default server).

    Kontrak:
    - Validasi `name`/`description` di `__init_subclass__` (fail eksplisit
      saat class didefinisikan, bukan saat registrasi).
    - `handle(self, input: dict) -> Any` adalah titik eksekusi — output
      HARUS JSON-serializable namun validasi di runtime handler, bukan registry.
    - `__tool_definition__()` mengembalikan shape golden 6-key dengan
      deepcopy + normalize (reuse logic registry).
    - `__tool_handler__()` mengembalikan callable `(dict) -> Any`.
    - Marker `__antikythera_tool__ = True` ditempel tanpa global auto-registry.
    """

    name: str = ""
    description: str = ""
    title: Optional[str] = None
    input_schema: Dict[str, Any] = DEFAULT_INPUT_SCHEMA
    output_schema: Optional[Dict[str, Any]] = None
    owner: str = "server"

    def handle(self, input: Dict[str, Any]) -> Any:  # noqa: A003
        """Eksekusi tool; subclass wajib override."""
        raise NotImplementedError(f"tool '{type(self).__name__}.handle' not implemented")

    def __init_subclass__(cls, **kwargs: Any) -> None:
        super().__init_subclass__(**kwargs)
        name = getattr(cls, "name", None)
        if not isinstance(name, str) or not name:
            raise ValueError("BaseTool: 'name' must be a non-empty string")
        description = getattr(cls, "description", None)
        if not isinstance(description, str) or not description:
            raise ValueError("BaseTool: 'description' must be a non-empty string")
        title = getattr(cls, "title", None)
        if title is not None and not isinstance(title, str):
            raise TypeError("BaseTool: 'title' must be a string or None")
        owner = getattr(cls, "owner", "server")
        if not isinstance(owner, str) or owner not in _VALID_OWNERS:
            raise ValueError(
                f"BaseTool: 'owner' must be one of {sorted(_VALID_OWNERS)}, got {owner!r}"
            )
        input_schema = getattr(cls, "input_schema", None)
        if input_schema is not None and not isinstance(input_schema, dict):
            raise TypeError("BaseTool: 'input_schema' must be a dict or None")
        output_schema = getattr(cls, "output_schema", None)
        if output_schema is not None and not isinstance(output_schema, dict):
            raise TypeError("BaseTool: 'output_schema' must be a dict or None")
        parameters = getattr(cls, "parameters", None)
        if parameters is not None and not isinstance(parameters, list):
            raise TypeError("BaseTool: 'parameters' must be a list or None")
        cls.__antikythera_tool__ = True  # type: ignore[attr-defined]

    @classmethod
    def __tool_definition__(cls) -> Dict[str, Any]:
        """Shape golden 6-key (deepcopy + normalize, tanpa owner)."""
        raw: Dict[str, Any] = {
            "name": getattr(cls, "name", None),
            "title": getattr(cls, "title", None),
            "description": getattr(cls, "description", None),
            "parameters": getattr(cls, "parameters", None),
            "input_schema": getattr(cls, "input_schema", None),
            "output_schema": getattr(cls, "output_schema", None),
        }
        return _normalize_definition(raw)

    @classmethod
    def __tool_handler__(cls) -> Callable[[Dict[str, Any]], Any]:
        """Handler terikat: instansiasi per panggilan lalu `handle(input)`."""

        def handler(tool_input: Dict[str, Any]) -> Any:
            instance = cls()
            return instance.handle(tool_input)

        return handler
