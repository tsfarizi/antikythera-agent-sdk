"""Falsification suite untuk `antikythera_agent.server.registry` (unit U12).

Verifikasi mekanis wajib kontrak U12: membuktikan registrasi normal,
`owner_of` benar, collision ditolak dengan pesan memuat nama tool, dan
`definitions()` ber-shape golden.

Sumber kebenaran:
- npm/antikythera-sdk/runtime/registry.js  (perilaku acuan JS: satu tool satu
  owner; collision lintas sisi = error eksplisit; re-registrasi owner sama =
  replace)
- antikythera-server-runtime/src/registry.rs (R5: pesan collision kanonik;
  definisi union diurutkan per nama)
- contracts/shared/wire_protocol.golden.json (shape ToolDefinition pada
  `tools_list_response` / `registry_sync_event.payload`: kunci persis
  {name, title, description, parameters, input_schema, output_schema} —
  tanpa kunci di luar golden)

Amplop test (asumsi yang dideklarasikan agar sertifikasi tetap sah):
- Jenis exception collision/validasi adalah `ValueError` (keputusan
  implementasi; klausa kontrak hanya mensyaratkan error eksplisit dengan
  pesan memuat "collision" dan nama tool).
- `definitions()` mengembalikan salinan — mutasi hasil tidak memengaruhi
  registry (invarian isolasi state registry-sync).
- Owner diuji terhadap nilai `WIRE` (`server`/`client`/`mcp`), bukan string
  literal hardcoded.

Menjalankan (dari repo root):
    $env:PYTHONPATH="python"
    python -m pytest python/tests/test_registry.py -v

Collision rejection (invarian: satu tool satu owner).

Definitions() shape golden — invarian: nol field di luar golden.

Handler server (optional; dieksekusi server-side).
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from antikythera_agent.server.registry import UnionRegistry
from antikythera_agent.server.wire import WIRE

_GOLDEN_PATH = (
    Path(__file__).resolve().parents[2] / "contracts" / "shared" / "wire_protocol.golden.json"
)

GOLDEN = json.loads(_GOLDEN_PATH.read_text(encoding="utf-8"))

#: Kunci kanonik item ToolDefinition (golden `tools_list_response`).
TOOL_DEFINITION_CANONICAL_KEYS = {
    "name",
    "title",
    "description",
    "parameters",
    "input_schema",
    "output_schema",
}

#: Owner valid dari wire.py (sumber kebenaran, bukan string hardcoded).
OWNER_SERVER = WIRE["OWNER_SERVER"]
OWNER_CLIENT = WIRE["OWNER_CLIENT"]
OWNER_MCP = WIRE["OWNER_MCP"]


def make_registry() -> UnionRegistry:
    """Registry dengan satu tool di tiap sisi, nama unik lintas sisi."""
    registry = UnionRegistry()
    registry.register_server({"name": "server_tool", "description": "runs on server"})
    registry.register_client({"name": "client_tool", "description": "runs on peer"})
    registry.register_mcp({"name": "mcp_tool", "description": "runs via mcp"})
    return registry



def test_register_three_sides_produce_single_union():
    """Registrasi normal: tool dari tiga sisi tergabung menjadi satu union."""
    registry = make_registry()
    assert registry.size() == 3
    names = [d["name"] for d in registry.definitions()]
    assert set(names) == {"server_tool", "client_tool", "mcp_tool"}


def test_definitions_are_sorted_by_name_for_determinism():
    """definitions(): urutan dijamin deterministik — sortir per nama (R5)."""
    registry = make_registry()
    names = [d["name"] for d in registry.definitions()]
    assert names == sorted(names)


def test_definitions_empty_when_nothing_registered():
    """definitions(): registry kosong mengembalikan list kosong."""
    registry = UnionRegistry()
    assert registry.definitions() == []
    assert registry.size() == 0



def test_owner_of_resolves_each_side():
    """owner_of: setiap tool ter-resolve ke sisi pemiliknya."""
    registry = make_registry()
    assert registry.owner_of("server_tool") == OWNER_SERVER
    assert registry.owner_of("client_tool") == OWNER_CLIENT
    assert registry.owner_of("mcp_tool") == OWNER_MCP


def test_owner_of_unknown_name_returns_none():
    """owner_of: nama yang tidak terdaftar mengembalikan None."""
    registry = make_registry()
    assert registry.owner_of("does_not_exist") is None


def test_has_reports_presence():
    """has: true hanya untuk nama terdaftar."""
    registry = make_registry()
    assert registry.has("server_tool")
    assert not registry.has("does_not_exist")



def test_cross_side_collision_is_rejected_with_name_in_message():
    """Collision lintas sisi: registrasi nama sama dari sisi lain DITOLAK
    dengan error eksplisit; pesan memuat "collision" dan nama tool."""
    registry = UnionRegistry()
    registry.register_server({"name": "get_current_time", "description": "server time"})
    with pytest.raises(ValueError) as excinfo:
        registry.register_client({"name": "get_current_time", "description": "peer time"})
    message = str(excinfo.value)
    assert "collision" in message
    assert "get_current_time" in message


def test_collision_does_not_overwrite_original_owner():
    """Collision ditolak: entri asli tidak tersentuh (bukan overwrite senyap)."""
    registry = UnionRegistry()
    registry.register_server({"name": "get_current_time", "description": "server time"})
    with pytest.raises(ValueError):
        registry.register_client({"name": "get_current_time", "description": "peer time"})
    with pytest.raises(ValueError):
        registry.register_mcp({"name": "get_current_time", "description": "mcp time"})
    assert registry.size() == 1
    assert registry.owner_of("get_current_time") == OWNER_SERVER
    assert registry.definitions()[0]["description"] == "server time"


def test_same_owner_reregistration_replaces_entry():
    """Re-registrasi pada owner yang SAMA menggantikan entri (mirror JS/Rust)."""
    registry = UnionRegistry()
    registry.register_server({"name": "echo", "description": "old"})
    registry.register_server({"name": "echo", "description": "new", "title": "New Echo"})
    assert registry.size() == 1
    assert registry.owner_of("echo") == OWNER_SERVER
    defs = registry.definitions()
    assert defs[0]["description"] == "new"
    assert defs[0]["title"] == "New Echo"


def test_definition_without_name_is_rejected():
    """Definisi tanpa nama (atau nama kosong) ditolak (mirror JS)."""
    registry = UnionRegistry()
    with pytest.raises(ValueError):
        registry.register_server({"description": "no name"})
    with pytest.raises(ValueError):
        registry.register_client({"name": "", "description": "empty name"})


def test_definition_without_description_is_rejected():
    """Definisi tanpa description ditolak (name dan description wajib)."""
    registry = UnionRegistry()
    with pytest.raises(ValueError):
        registry.register_server({"name": "t"})
    with pytest.raises(ValueError):
        registry.register_mcp({"name": "t", "description": None})



def test_definitions_shape_matches_golden_tools_list_response():
    """definitions(): setiap item memuat PERSIS kunci kanonik golden — tanpa
    kunci di luar golden (invarian 5 pada jalur registry-sync / GET /tools)."""
    registry = make_registry()
    for item in registry.definitions():
        assert isinstance(item, dict)
        assert set(item.keys()) == TOOL_DEFINITION_CANONICAL_KEYS, (
            f"ToolDefinition keys {sorted(item)} must equal canonical "
            f"{sorted(TOOL_DEFINITION_CANONICAL_KEYS)}"
        )


def test_definition_defaults_match_golden_input_schema():
    """ToolDefinition: kunci opsional memakai default golden saat tidak
    diberikan — input_schema object-schema kosong, parameters [], title None,
    output_schema None."""
    registry = UnionRegistry()
    registry.register_server({"name": "get_current_time", "description": "Get time."})
    item = registry.definitions()[0]
    assert item["name"] == "get_current_time"
    assert item["description"] == "Get time."
    assert item["title"] is None
    assert item["parameters"] == []
    assert item["input_schema"] == {"type": "object", "properties": {}, "required": []}
    assert item["output_schema"] is None


def test_definition_preserves_given_optional_fields():
    """ToolDefinition: title/parameters/input_schema/output_schema yang
    DIBERIKAN dipertahankan apa adanya (mirror `??` — falsy non-None tidak
    diganti default)."""
    registry = UnionRegistry()
    input_schema = {
        "type": "object",
        "properties": {"query": {"type": "string"}},
        "required": ["query"],
    }
    registry.register_mcp(
        {
            "name": "search",
            "title": "Search",
            "description": "Search the index.",
            "parameters": [{"name": "query", "type": "string"}],
            "input_schema": input_schema,
            "output_schema": {"type": "object"},
        }
    )
    item = registry.definitions()[0]
    assert item == {
        "name": "search",
        "title": "Search",
        "description": "Search the index.",
        "parameters": [{"name": "query", "type": "string"}],
        "input_schema": input_schema,
        "output_schema": {"type": "object"},
    }


def test_definitions_return_copies_so_mutation_does_not_leak():
    """definitions(): hasil adalah salinan — mutasi item hasil tidak
    memengaruhi registry (isolasi state untuk registry-sync / GET /tools)."""
    registry = make_registry()
    item = registry.definitions()[0]
    item["name"] = "mutated"
    item["description"] = "mutated"
    assert registry.has("mutated") is False
    assert registry.owner_of("mutated") is None
    assert registry.size() == 3
    assert "mutated" not in [d["name"] for d in registry.definitions()]



def test_server_handler_is_stored_and_exposed():
    """register_server: handler optional disimpan dan tersedia via handler_of."""
    calls = []

    def handler(arguments):
        calls.append(arguments)
        return "ok"

    registry = UnionRegistry()
    registry.register_server({"name": "echo", "description": "Echo input."}, handler=handler)
    assert registry.handler_of("echo") is handler
    registry.handler_of("echo")({"message": "hi"})
    assert calls == [{"message": "hi"}]


def test_client_mcp_and_unknown_tools_have_no_handler():
    """handler_of: tool client/mcp (definisi saja) dan nama tak dikenal
    mengembalikan None."""
    registry = make_registry()
    assert registry.handler_of("client_tool") is None
    assert registry.handler_of("mcp_tool") is None
    assert registry.handler_of("does_not_exist") is None
