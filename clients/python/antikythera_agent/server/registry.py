"""Union tool registry (R5): server + client + MCP tool definitions.

Sumber kebenaran:
- npm/antikythera-sdk/runtime/registry.js  (perilaku acuan JS: satu tool satu
  owner; collision lintas sisi = error eksplisit; re-registrasi owner yang
  sama = replace)
- antikythera-server-runtime/src/registry.rs (R5: pesan collision kanonik;
  definisi union diurutkan per nama untuk determinisme)
- contracts/shared/wire_protocol.golden.json (shape ToolDefinition pada
  `tools_list_response` / `registry_sync_event.payload` — kunci persis, tanpa
  kunci di luar golden)

Semantik: registry ini adalah UNION tool dari tiga sisi — `server` (lokal
server), `client` (peer client), `mcp` (MCP server; selalu dieksekusi
server-side). Setiap tool memiliki TEPAT SATU owner; registrasi nama yang
sudah dimiliki sisi lain adalah error eksplisit, bukan overwrite senyap.
"""

from __future__ import annotations

import copy
from dataclasses import dataclass
from typing import Any, Callable, Dict, List, Optional

from .wire import WIRE

#: Default schema input tool (golden `input_schema` saat tidak diberikan).
DEFAULT_INPUT_SCHEMA = {"type": "object", "properties": {}, "required": []}


@dataclass
class _ToolEntry:
    owner: str
    definition: Dict[str, Any]
    handler: Optional[Callable[..., Any]] = None


def _normalize_definition(definition: Any) -> Dict[str, Any]:
    """Normalisasi definisi tool ke shape golden (6 kunci kanonik).

    `name` dan `description` wajib (ValueError bila absen/None/kosong);
    `title`/`output_schema` default None; `parameters` default [];
    `input_schema` default object-schema kosong. Nilai yang DIBERIKAN
    dipertahankan apa adanya (mirror `??` — None diganti default, falsy
    non-None tidak). Semua nilai disalin agar mutasi pemanggil tidak bocor
    ke registry.
    """
    if not isinstance(definition, dict):
        raise ValueError("registry: tool definition requires an object")
    name = definition.get("name")
    if not isinstance(name, str) or not name:
        raise ValueError("registry: tool definition requires a name")
    description = definition.get("description")
    if not isinstance(description, str) or not description:
        raise ValueError("registry: tool definition requires a description")
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


class UnionRegistry:
    """Union tool registry dengan ownership eksklusif per sisi.

    Setiap tool terdaftar persis pada satu owner di {server, client, mcp}.
    Registrasi nama yang sudah dimiliki sisi lain menimbulkan ValueError
    dengan pesan kanonik R5; re-registrasi pada owner yang sama menggantikan
    entri lama (mirror JS `byName.set` dan Rust `insert`).
    """

    def __init__(self) -> None:
        self._tools: Dict[str, _ToolEntry] = {}

    def register_server(
        self,
        definition: Dict[str, Any],
        handler: Optional[Callable[..., Any]] = None,
    ) -> None:
        """Daftarkan tool yang dieksekusi server-side (owner `server`)."""
        self._register(WIRE["OWNER_SERVER"], definition, handler)

    def register_client(self, definition: Dict[str, Any]) -> None:
        """Daftarkan definisi tool yang dieksekusi peer client (owner `client`)."""
        self._register(WIRE["OWNER_CLIENT"], definition)

    def register_mcp(
        self, definition: Dict[str, Any], handler: Optional[Callable[..., Any]] = None
    ) -> None:
        """Daftarkan definisi tool MCP; dieksekusi server-side (owner `mcp`)."""
        self._register(WIRE["OWNER_MCP"], definition, handler)

    def _register(
        self,
        owner: str,
        definition: Dict[str, Any],
        handler: Optional[Callable[..., Any]] = None,
    ) -> None:
        normalized = _normalize_definition(definition)
        name = normalized["name"]
        existing = self._tools.get(name)
        if existing is not None and existing.owner != owner:
            raise ValueError(
                f"tool registry: name collision for tool '{name}' "
                f"(owners {existing.owner}, {owner})"
            )
        self._tools[name] = _ToolEntry(owner=owner, definition=normalized, handler=handler)

    def owner_of(self, name: str) -> Optional[str]:
        """Owner tool (`server`/`client`/`mcp`) atau None bila tidak terdaftar."""
        entry = self._tools.get(name)
        return entry.owner if entry is not None else None

    def has(self, name: str) -> bool:
        return name in self._tools

    def size(self) -> int:
        return len(self._tools)

    def handler_of(self, name: str) -> Optional[Callable[..., Any]]:
        """Handler server tool, atau None (tool client/mcp/unknown tidak punya)."""
        entry = self._tools.get(name)
        return entry.handler if entry is not None else None

    def definitions(self) -> List[Dict[str, Any]]:
        """Union definisi tool shape golden, diurutkan per nama (determinisme R5).

        Setiap item PERSIS memuat kunci kanonik golden — tidak ada kunci lain.
        Item adalah salinan; mutasi hasil oleh konsumen tidak memengaruhi
        registry (menjaga registry-sync / GET /tools dari polusi state).
        """
        definitions = [copy.deepcopy(entry.definition) for entry in self._tools.values()]
        definitions.sort(key=lambda d: d["name"])
        return definitions
