"""Falsification suite untuk `antikythera_agent.server.wire` (unit E03, tugas U03-T).

Peran Tester — disiplin tdd-cycle red/green. File ini MENDEFINISIKAN kontrak
modul wire.py yang AKAN diimplementasikan oleh Coder; ia tidak menguji
implementasi internal, hanya kontrak yang dapat diamati melalui port publik
modul. Sebelum modul ada, suite berhenti pada collection error
(ModuleNotFoundError) — itulah state RED yang sah.

Sumber kebenaran:
- contracts/shared/wire_protocol.golden.json  (shape kanonik; kunci wajib persis)
- npm/antikythera-sdk/runtime/types.js         (referensi perilaku JS yang sudah
  ada; test Python mencerminkan perilaku yang sama — strict boolean `=== true`,
  nullish-default `??`, streaming BUKAN di body)
- documentation/WIRE_PROTOCOL.md  §2.1 (streaming via query param `?stream=true`,
  bukan metadata_json) dan §6 (mapping wire tool-execution-result → runner
  ToolResultInput; step_id di-drop; output_json wajib).

Amplop test (asumsi yang dideklarasikan agar sertifikasi tetap sah):
- Input builder memakai kunci snake_case (terjemahan Python dari camelCase JS:
  sessionId → session_id, messagesJson → messages_json, dst).
- Klausa "input non-object → raise" hanya mensyaratkan munculnya exception;
  jenis exception spesifik TIDAK ditentukan oleh E03, jadi test memakai
  `pytest.raises(Exception)`.
- Argumen korrelasi `wire_to_runner_tool_result` dipanggil secara POSITIONAL
  (nama keyword tidak dikontrak oleh E03).
- tools_list_response tidak punya builder ToolDefinition di API wire.py (E03
  tidak mencantumkannya); konformansinya diuji sebagai shape-lock item kanonik.

Menjalankan (dari repo root):
    $env:PYTHONPATH="python"
    python -m pytest python/tests/test_wire.py -v

Golden contract loader (sumber kebenaran shape; read-only).

Golden conformance table: setiap shape yang DAPAT dibangun/di-parse oleh wire.py dipetakan ke
builder/parser-nya. Input sedapat mungkin memakai sampel golden langsung sehingga test
mendokumentasikan diri.

Build_llm_request — output snake_case, golden llm_call_request.

Parse_llm_response — golden llm_call_response; non-object → raise.

Build_tool_call_event — output kebab-case, golden tool_execute_request.

Parse_tool_execution_result — golden tool_execute_response; non-object → raise.

Parse_event_envelope — golden *_event; non-object → raise.

Build_postback — golden postback_response / postback_gate_denial.

Wire_to_runner_tool_result — §6 mapping; step_id di-drop; output_json wajib.

Golden conformance — invarian 5: TIDAK BOLEH ada kunci di luar golden.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

import antikythera_agent.server.wire as wire


_GOLDEN_PATH = (
    Path(__file__).resolve().parents[2] / "contracts" / "shared" / "wire_protocol.golden.json"
)

GOLDEN = json.loads(_GOLDEN_PATH.read_text(encoding="utf-8"))

#: Input yang BUKAN objek (dict) — klausa "non-object → raise" pada parser.
NON_OBJECT_INPUTS = [None, [], "text", 42, 3.14, True]

#: Kunci kanonik item ToolDefinition (golden `tools_list_response`).
TOOL_DEFINITION_CANONICAL_KEYS = {
    "name",
    "title",
    "description",
    "parameters",
    "input_schema",
    "output_schema",
}


GOLDEN_CONFORMANCE_CASES = [
    ("llm_call_request", "build_llm_request", GOLDEN["llm_call_request"]),
    ("llm_call_response", "parse_llm_response", GOLDEN["llm_call_response"]),
    (
        "tool_execute_request",
        "build_tool_call_event",
        {"tool_name": "get_current_time", "arguments_json": "{}", "session_id": "session-123", "step_id": 1},
    ),
    ("tool_execute_response", "parse_tool_execution_result", GOLDEN["tool_execute_response"]),
    ("tool_execution_request_event", "parse_event_envelope", GOLDEN["tool_execution_request_event"]),
    ("hook_request_event", "parse_event_envelope", GOLDEN["hook_request_event"]),
    ("llm_token_event", "parse_event_envelope", GOLDEN["llm_token_event"]),
    ("registry_sync_event", "parse_event_envelope", GOLDEN["registry_sync_event"]),
    ("lifecycle_event", "parse_event_envelope", GOLDEN["lifecycle_event"]),
    ("error_event", "parse_event_envelope", GOLDEN["error_event"]),
    ("postback_response", "build_postback", GOLDEN["postback_response"]),
    ("postback_gate_denial", "build_postback", GOLDEN["postback_gate_denial"]),
]



def test_wire_constants_endpoint_paths_match_canonical():
    """WIRE: keempat path endpoint sama persis dengan nilai kanonik (§2)."""
    expected = {
        "LLM_CALL": "/antikythera/v1/llm/call",
        "TOOLS_EXECUTE": "/antikythera/v1/tools/execute",
        "TOOLS_LIST": "/antikythera/v1/tools",
        "EVENTS": "/antikythera/v1/events",
    }
    actual = {key: wire.WIRE[key] for key in expected}
    assert actual == expected


def test_wire_constants_owners_match_canonical():
    """WIRE: nilai owner registry sama persis dengan kanonik (`client`/`server`/`mcp`)."""
    expected = {
        "OWNER_CLIENT": "client",
        "OWNER_SERVER": "server",
        "OWNER_MCP": "mcp",
    }
    actual = {key: wire.WIRE[key] for key in expected}
    assert actual == expected


def test_wire_constants_hooks_match_canonical():
    """WIRE: nama hook runtime sama persis dengan kanonik (§3.2)."""
    expected = {
        "HOOK_PREPARE_TURN": "prepare-turn",
        "HOOK_DECIDE_ACTION": "decide-action",
        "HOOK_HANDLE_TOOL_RESULT": "handle-tool-result",
    }
    actual = {key: wire.WIRE[key] for key in expected}
    assert actual == expected


def test_wire_constants_passthrough_matches_canonical():
    """WIRE: PASSTHROUGH adalah keputusan passthrough hook yang persis."""
    assert wire.WIRE["PASSTHROUGH"] == '{"passthrough": true}'



def test_build_llm_request_maps_input_to_snake_case_contract_fields():
    """build_llm_request: semua field input dipetakan ke nama snake_case kanonik."""
    req = wire.build_llm_request(
        {
            "provider": "ollama",
            "model": "gpt-oss:120b-cloud",
            "session_id": "session-123",
            "messages_json": '[{"role":"user","content":"hi"}]',
            "force_json": False,
            "temperature": 0.7,
            "max_tokens": 512,
            "schema_name": None,
            "metadata_json": None,
        }
    )
    assert req == {
        "provider": "ollama",
        "model": "gpt-oss:120b-cloud",
        "session_id": "session-123",
        "messages_json": '[{"role":"user","content":"hi"}]',
        "force_json": False,
        "temperature": 0.7,
        "max_tokens": 512,
        "schema_name": None,
        "metadata_json": None,
    }


def test_build_llm_request_applies_contract_defaults_when_fields_missing():
    """build_llm_request: field yang tidak diberikan memakai default None/''/False."""
    req = wire.build_llm_request({})
    assert req == {
        "provider": None,
        "model": None,
        "session_id": None,
        "messages_json": "",
        "force_json": False,
        "temperature": None,
        "max_tokens": None,
        "schema_name": None,
        "metadata_json": None,
    }


def test_build_llm_request_preserves_provided_falsy_values():
    """build_llm_request: nilai falsy yang DIBERIKAN (0.0, 0, False, '') tetap
    dipertahankan (mirror `??` JS — bukan `or`-coalescing)."""
    req = wire.build_llm_request({"force_json": False, "temperature": 0.0, "max_tokens": 0, "messages_json": ""})
    assert req["force_json"] is False
    assert req["temperature"] == 0.0
    assert req["max_tokens"] == 0
    assert req["messages_json"] == ""


def test_build_llm_request_does_not_emit_stream_key_in_body():
    """build_llm_request: objek request TIDAK boleh memuat kunci `stream`
    (streaming disinyalkan lewat query param, bukan body — §2.1)."""
    req = wire.build_llm_request({"metadata_json": '{"x": 1}'})
    assert "stream" not in req


def test_build_llm_request_keeps_metadata_json_verbatim_without_stream_flag():
    """build_llm_request: metadata_json diteruskan apa adanya; flag streaming
    tidak boleh disuntikkan ke dalam metadata_json."""
    raw = '{"x": 1}'
    req = wire.build_llm_request({"metadata_json": raw})
    assert req["metadata_json"] == raw
    metadata = json.loads(req["metadata_json"])
    assert "stream" not in metadata



def test_parse_llm_response_of_golden_sample_returns_identical_object():
    """parse_llm_response: sampel golden di-parse menjadi objek identik (tidak
    menambah/menghapus/mengubah nilai)."""
    assert wire.parse_llm_response(GOLDEN["llm_call_response"]) == GOLDEN["llm_call_response"]


@pytest.mark.parametrize("non_object", NON_OBJECT_INPUTS, ids=lambda v: type(v).__name__)
def test_parse_llm_response_raises_on_non_object_input(non_object):
    """parse_llm_response: input non-object WAJIB menimbulkan exception."""
    with pytest.raises(Exception):
        wire.parse_llm_response(non_object)


def test_parse_llm_response_applies_defaults_when_fields_missing():
    """parse_llm_response: field yang hilang memakai default (content='', sisanya None)."""
    assert wire.parse_llm_response({}) == {
        "content": "",
        "model": None,
        "session_id": None,
        "message_json": None,
        "tokens_used": None,
        "finish_reason": None,
        "raw_response_json": None,
    }


def test_parse_llm_response_coerces_non_string_content_to_empty_string():
    """parse_llm_response: content non-string (angka, objek, null) dikosongkan
    menjadi '' (mirror `typeof body.content === 'string'`)."""
    assert wire.parse_llm_response({"content": 123})["content"] == ""
    assert wire.parse_llm_response({"content": {"nested": True}})["content"] == ""
    assert wire.parse_llm_response({"content": None})["content"] == ""


def test_parse_llm_response_preserves_provided_falsy_values():
    """parse_llm_response: nilai falsy yang DIBERIKAN ('' dan 0) dipertahankan."""
    parsed = wire.parse_llm_response({"content": "", "tokens_used": 0})
    assert parsed["content"] == ""
    assert parsed["tokens_used"] == 0



def test_build_tool_call_event_maps_input_to_kebab_case_contract_fields():
    """build_tool_call_event: semua field dipetakan ke nama kebab-case kanonik."""
    evt = wire.build_tool_call_event(
        {"tool_name": "get_current_time", "arguments_json": "{}", "session_id": "session-123", "step_id": 1}
    )
    assert evt == {
        "tool-name": "get_current_time",
        "arguments-json": "{}",
        "session-id": "session-123",
        "step-id": 1,
    }


def test_build_tool_call_event_applies_defaults_when_fields_missing():
    """build_tool_call_event: default untuk arguments-json='{}', session-id=None, step-id=0."""
    evt = wire.build_tool_call_event({"tool_name": "get_current_time"})
    assert evt == {
        "tool-name": "get_current_time",
        "arguments-json": "{}",
        "session-id": None,
        "step-id": 0,
    }


def test_build_tool_call_event_preserves_provided_empty_arguments():
    """build_tool_call_event: arguments_json='' yang DIBERIKAN dipertahankan
    (bukan diganti default '{}'); step-id=0 dipertahankan."""
    evt = wire.build_tool_call_event({"tool_name": "get_current_time", "arguments_json": "", "step_id": 0})
    assert evt["arguments-json"] == ""
    assert evt["step-id"] == 0



def test_parse_tool_execution_result_of_golden_sample_returns_identical_object():
    """parse_tool_execution_result: sampel golden di-parse menjadi objek identik."""
    assert (
        wire.parse_tool_execution_result(GOLDEN["tool_execute_response"])
        == GOLDEN["tool_execute_response"]
    )


@pytest.mark.parametrize("non_object", NON_OBJECT_INPUTS, ids=lambda v: type(v).__name__)
def test_parse_tool_execution_result_raises_on_non_object_input(non_object):
    """parse_tool_execution_result: input non-object WAJIB menimbulkan exception."""
    with pytest.raises(Exception):
        wire.parse_tool_execution_result(non_object)


def test_parse_tool_execution_result_applies_defaults_when_fields_missing():
    """parse_tool_execution_result: default untuk tool-name='', success=False,
    output-json='{}', error-message=None, step-id=0."""
    assert wire.parse_tool_execution_result({}) == {
        "tool-name": "",
        "success": False,
        "output-json": "{}",
        "error-message": None,
        "step-id": 0,
    }


def test_parse_tool_execution_result_success_is_strict_boolean():
    """parse_tool_execution_result: `success` hanya True untuk boolean True
    sejati (mirror `=== true`); truthy non-bool dipaksa False."""
    assert wire.parse_tool_execution_result({"tool-name": "t", "success": "true"})["success"] is False
    assert wire.parse_tool_execution_result({"tool-name": "t", "success": 1})["success"] is False
    assert wire.parse_tool_execution_result({"tool-name": "t", "success": True})["success"] is True


def test_parse_tool_execution_result_preserves_provided_falsy_values():
    """parse_tool_execution_result: falsy yang DIBERIKAN (False, '', 0)
    dipertahankan, bukan diganti default."""
    parsed = wire.parse_tool_execution_result(
        {"tool-name": "t", "success": False, "output-json": "", "error-message": "", "step-id": 0}
    )
    assert parsed["success"] is False
    assert parsed["output-json"] == ""
    assert parsed["error-message"] == ""
    assert parsed["step-id"] == 0



def test_parse_event_envelope_of_golden_sample_returns_identical_object():
    """parse_event_envelope: envelope golden (tool_execution_request_event)
    di-parse menjadi objek identik."""
    assert (
        wire.parse_event_envelope(GOLDEN["tool_execution_request_event"])
        == GOLDEN["tool_execution_request_event"]
    )


@pytest.mark.parametrize("non_object", NON_OBJECT_INPUTS, ids=lambda v: type(v).__name__)
def test_parse_event_envelope_raises_on_non_object_input(non_object):
    """parse_event_envelope: input non-object WAJIB menimbulkan exception."""
    with pytest.raises(Exception):
        wire.parse_event_envelope(non_object)


def test_parse_event_envelope_applies_defaults_when_fields_missing():
    """parse_event_envelope: field yang hilang menjadi None; `type` ikut
    diteruskan (None bila tidak ada)."""
    assert wire.parse_event_envelope({}) == {
        "type": None,
        "correlation_id": None,
        "session_id": None,
        "client_id": None,
        "payload": None,
    }


def test_parse_event_envelope_preserves_provided_falsy_values():
    """parse_event_envelope: falsy yang DIBERIKAN ('' dan []) dipertahankan."""
    env = wire.parse_event_envelope(
        {"type": "lifecycle", "correlation_id": "", "session_id": None, "client_id": "", "payload": []}
    )
    assert env["correlation_id"] == ""
    assert env["client_id"] == ""
    assert env["payload"] == []



def test_build_postback_maps_input_to_contract_fields():
    """build_postback: correlation_id, ok, payload, error dipetakan persis."""
    payload = GOLDEN["postback_response"]["payload"]
    pb = wire.build_postback({"correlation_id": "corr-0001", "ok": True, "payload": payload, "error": None})
    assert pb == {
        "correlation_id": "corr-0001",
        "ok": True,
        "payload": payload,
        "error": None,
    }


def test_build_postback_ok_is_strict_boolean():
    """build_postback: `ok` hanya True untuk boolean True sejati (mirror `=== true`)."""
    assert wire.build_postback({"correlation_id": "c", "ok": True})["ok"] is True
    assert wire.build_postback({"correlation_id": "c", "ok": "true"})["ok"] is False
    assert wire.build_postback({"correlation_id": "c", "ok": 1})["ok"] is False
    assert wire.build_postback({"correlation_id": "c", "ok": None})["ok"] is False


def test_build_postback_applies_defaults_when_fields_missing():
    """build_postback: default untuk ok=False, payload=None, error=None."""
    assert wire.build_postback({"correlation_id": "c"}) == {
        "correlation_id": "c",
        "ok": False,
        "payload": None,
        "error": None,
    }



def test_wire_to_runner_tool_result_maps_wire_fields_and_drops_step_id():
    """wire_to_runner_tool_result: hasil wire kebab-case dipetakan ke runner
    snake_case; step_id TIDAK ikut (di-drop); correlation_id diteruskan."""
    result = wire.wire_to_runner_tool_result(
        {
            "tool-name": "get_current_time",
            "success": True,
            "output-json": '{"datetime":"2026-08-12T00:00:00Z"}',
            "error-message": None,
            "step-id": 1,
        },
        "corr-0001",
    )
    assert result == {
        "tool_name": "get_current_time",
        "success": True,
        "output_json": '{"datetime":"2026-08-12T00:00:00Z"}',
        "error_message": None,
        "correlation_id": "corr-0001",
    }
    assert "step_id" not in result


def test_wire_to_runner_tool_result_emits_exact_runner_shape():
    """wire_to_runner_tool_result: kunci output PERSIS {tool_name, success,
    output_json, error_message, correlation_id} (invarian §6 — tanpa kunci lain)."""
    result = wire.wire_to_runner_tool_result(
        {"tool-name": "t", "success": True, "output-json": "{}", "error-message": None, "step-id": 9},
        "corr-1",
    )
    assert set(result.keys()) == {"tool_name", "success", "output_json", "error_message", "correlation_id"}


def test_wire_to_runner_tool_result_defaults_correlation_id_success_and_optional_fields():
    """wire_to_runner_tool_result: correlation_id default None; success default
    False; output_json default '{}' (WAJIB ada) dan error_message default None."""
    result = wire.wire_to_runner_tool_result({"tool-name": "t"})
    assert result["correlation_id"] is None
    assert result["success"] is False
    assert result["output_json"] == "{}"
    assert result["error_message"] is None


def test_wire_to_runner_tool_result_success_is_strict_boolean():
    """wire_to_runner_tool_result: `success` hanya True untuk boolean True
    sejati (mirror `=== true`)."""
    assert wire.wire_to_runner_tool_result({"tool-name": "t", "success": "true"})["success"] is False
    assert wire.wire_to_runner_tool_result({"tool-name": "t", "success": 1})["success"] is False
    assert wire.wire_to_runner_tool_result({"tool-name": "t", "success": True})["success"] is True



@pytest.mark.parametrize(
    "shape_name,function_name,input_value",
    GOLDEN_CONFORMANCE_CASES,
    ids=[case[0] for case in GOLDEN_CONFORMANCE_CASES],
)
def test_golden_shape_has_no_extra_keys(shape_name, function_name, input_value):
    """Konformansi golden: objek yang dibangun/di-parse wire.py untuk shape
    `shape_name` memiliki kunci PERSIS sama dengan sampel golden (mirror
    assertNoExtraKeys + assertHasKeys dari runtime-bridge.test.mjs)."""
    golden_sample = GOLDEN[shape_name]
    fn = getattr(wire, function_name)
    actual = fn(input_value)
    assert isinstance(actual, dict), (
        f"{shape_name}: {function_name} must return a dict, got {type(actual).__name__}"
    )
    assert set(actual.keys()) == set(golden_sample.keys()), (
        f"{shape_name}: wire keys {sorted(actual)} must equal golden keys "
        f"{sorted(golden_sample)} (no extra keys; no missing keys)"
    )


def test_golden_tools_list_response_definition_shape_locked():
    """Konformansi golden untuk tools_list_response (item ToolDefinition).

    Amplop: E03 tidak mencantumkan builder ToolDefinition di API wire.py,
    sehingga tidak ada objek shape ini yang dibangun/di-parse oleh wire.py.
    Klausa konformansi tetap dieksekusi dengan mengunci shape kanonik item
    dari golden: setiap item wajib memuat PERSIS kunci kanonik
    {name, title, description, parameters, input_schema, output_schema} —
    menjaga sumber kebenaran dari penyisipan kunci yang akan melanggar
    invarian 5 pada jalur registry-sync / GET /tools.
    """
    samples = GOLDEN["tools_list_response"]
    assert isinstance(samples, list) and samples, "golden tools_list_response must be a non-empty array"
    for item in samples:
        assert isinstance(item, dict), "every ToolDefinition must be an object"
        assert set(item.keys()) == TOOL_DEFINITION_CANONICAL_KEYS, (
            f"ToolDefinition keys {sorted(item)} must equal canonical "
            f"{sorted(TOOL_DEFINITION_CANONICAL_KEYS)}"
        )
