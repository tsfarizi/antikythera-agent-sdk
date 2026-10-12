"""Verification suite untuk `antikythera_agent.server.provider` (unit U14).

Peran Coder — verifikasi mekanis WAJIB: suite ini memfalsifikasi kontrak
sambungan U14 yang diimplementasikan provider.py:

- `StubProvider` mengembalikan content yang dikonfigurasi dengan shape
  golden `llm_call_response` (persis tujuh kunci — nol field ekstra).
- `OllamaProvider` membangun request dari llm-request golden dan memetakan
  respons ke shape golden TANPA HTTP sungguhan (transport di-inject).
- `resolve_provider` memakai `provider_registry`; default provider `"stub"`.
- Error path: kegagalan LLM dibungkus ke `LlmError` (bukan exception HTTP
  mentah); input wire invalid -> `ValueError`.

Sumber kebenaran shape:
- contracts/shared/wire_protocol.golden.json (shape kanonik llm-request /
  llm-response; invarian nol field di luar golden — WIRE_PROTOCOL §7).

Amplop test (asumsi yang dideklarasikan agar sertifikasi tetap sah):
- Transport `OllamaProvider` di-inject sebagai callable
  `(url, body, timeout) -> parsed JSON object`; amplop ini menggantikan
  `urllib.request` sehingga tidak ada HTTP sungguhan.
- `metadata_json` TIDAK dikirim ke Ollama `/api/chat` (API-nya tidak punya
  field metadata) dan TIDAK pernah dibaca sebagai sinyal stream (invarian
  U14; sinyal stream = query param `?stream=true`, milik transport/control).
- `response_json` stub harus berupa JSON object; input invalid ditolak
  `ValueError` pada konstruksi (fail fast konfigurasi statis).

Menjalankan (dari repo root):
    $env:PYTHONPATH="python"
    python -m pytest python/tests/test_provider.py -v

Golden contract loader (sumber kebenaran shape; read-only).

Registry — provider_registry + resolve_provider + default "stub".

StubProvider — content terkonfigurasi; shape golden; fail fast konfigurasi.

Kontrak call — validasi input wire di titik masuk (entry point).

OllamaProvider — build request dari kontrak golden + mapping respons.

Error path — LlmError untuk kegagalan LLM; ValueError untuk input wire.
"""

from __future__ import annotations

import json
import urllib.error
from pathlib import Path

import pytest

import antikythera_agent.server.provider as provider


_GOLDEN_PATH = (
    Path(__file__).resolve().parents[2] / "contracts" / "shared" / "wire_protocol.golden.json"
)

GOLDEN = json.loads(_GOLDEN_PATH.read_text(encoding="utf-8"))

#: Kunci kanonik shape `llm_call_response` (nol field ekstra).
GOLDEN_LLM_RESPONSE_KEYS = set(GOLDEN["llm_call_response"].keys())

#: Sampel golden llm-request (salinan agar test tidak memutasi GOLDEN).
GOLDEN_LLM_REQUEST = dict(GOLDEN["llm_call_request"])


def _capturing_transport(captured):
    """Transport inject: menangkap URL/body/timeout, mengembalikan respons
    Ollama simulasi (content "Hello", tokens 1+3=4, done_reason "stop")."""

    def transport(url, body, timeout):
        captured["url"] = url
        captured["body"] = body
        captured["timeout"] = timeout
        return {
            "model": "gpt-oss:120b-cloud",
            "message": {"role": "assistant", "content": "Hello"},
            "done": True,
            "done_reason": "stop",
            "prompt_eval_count": 1,
            "eval_count": 3,
        }

    return transport



def test_default_provider_constant_is_stub():
    """DEFAULT_PROVIDER: default provider adalah `"stub"` (keputusan D-2)."""
    assert provider.DEFAULT_PROVIDER == "stub"


def test_provider_registry_contains_stub_and_ollama():
    """provider_registry: memuat persis entri `stub` dan `ollama`."""
    assert set(provider.provider_registry) == {"stub", "ollama"}


def test_resolve_provider_defaults_to_stub():
    """resolve_provider: tanpa argumen / None -> instansi stub registry."""
    assert provider.resolve_provider() is provider.provider_registry["stub"]
    assert provider.resolve_provider(None) is provider.provider_registry["stub"]
    assert isinstance(provider.resolve_provider(), provider.StubProvider)


def test_resolve_provider_returns_registered_instances():
    """resolve_provider: nama terdaftar mengembalikan instansi registry."""
    assert provider.resolve_provider("stub") is provider.provider_registry["stub"]
    assert provider.resolve_provider("ollama") is provider.provider_registry["ollama"]


def test_resolve_provider_unknown_name_raises_key_error():
    """resolve_provider: nama tak dikenal -> KeyError (lookup dict eksplisit)."""
    with pytest.raises(KeyError):
        provider.resolve_provider("unknown-provider")



def test_stub_provider_returns_configured_content_with_golden_shape():
    """StubProvider (parity Rust StubLlmProvider): content = SELURUH string
    response_json (bukan nilai field `content`); model/session_id dari
    REQUEST; hasil memakai shape golden `llm_call_response`."""
    stub = provider.StubProvider(
        '{"content": "Hello from stub", "model": "stub-model", '
        '"session_id": "session-123", "tokens_used": 4, "finish_reason": "stop"}'
    )
    result = stub.call({"model": "request-model", "session_id": "request-sid"})
    assert result["content"] == (
        '{"content": "Hello from stub", "model": "stub-model", '
        '"session_id": "session-123", "tokens_used": 4, "finish_reason": "stop"}'
    )
    assert result["model"] == "request-model"
    assert result["session_id"] == "request-sid"
    assert result["tokens_used"] == 4
    assert result["finish_reason"] == "stop"
    assert result["message_json"] is None
    assert result["raw_response_json"] is None
    assert set(result.keys()) == GOLDEN_LLM_RESPONSE_KEYS


def test_stub_provider_golden_sample_round_trip_is_identical():
    """StubProvider (parity Rust): sampel golden llm_call_response sebagai
    konfigurasi menghasilkan content = SELURUH string JSON sampel, bukan
    nilai field `content`; model/session_id diambil dari request (None saat
    request kosong); field lain mengikuti kontrak StubLlmProvider."""
    stub = provider.StubProvider(json.dumps(GOLDEN["llm_call_response"]))
    result = stub.call({})
    assert result["content"] == json.dumps(GOLDEN["llm_call_response"])
    assert result["model"] is None
    assert result["session_id"] is None
    assert result["message_json"] is None
    assert result["tokens_used"] == 4
    assert result["finish_reason"] == "stop"
    assert result["raw_response_json"] is None
    assert set(result.keys()) == GOLDEN_LLM_RESPONSE_KEYS


def test_stub_provider_drops_extra_fields_from_configured_json():
    """StubProvider (parity Rust): content = SELURUH string response_json;
    kunci di luar golden tetap dibuang oleh parser (invarian nol field
    ekstra — WIRE_PROTOCOL §7)."""
    stub = provider.StubProvider('{"content": "x", "extra_field": "must be dropped"}')
    result = stub.call({})
    assert set(result.keys()) == GOLDEN_LLM_RESPONSE_KEYS
    assert result["content"] == '{"content": "x", "extra_field": "must be dropped"}'


def test_stub_provider_rejects_invalid_response_json():
    """StubProvider: response_json bukan JSON valid -> ValueError (fail fast)."""
    with pytest.raises(ValueError):
        provider.StubProvider("{not json")


def test_stub_provider_rejects_non_object_response_json():
    """StubProvider: response_json JSON non-object (array) -> ValueError."""
    with pytest.raises(ValueError):
        provider.StubProvider('["array", "not", "object"]')



@pytest.mark.parametrize(
    "instance",
    [
        pytest.param(provider.StubProvider(), id="stub"),
        pytest.param(
            provider.OllamaProvider(transport=lambda url, body, timeout: {}),
            id="ollama",
        ),
    ],
)
def test_call_rejects_non_object_request(instance):
    """call: request non-object WAJIB menimbulkan ValueError (konvensi wire)."""
    with pytest.raises(ValueError):
        instance.call("not an object")



def test_ollama_provider_builds_request_from_golden_contract_shape():
    """OllamaProvider: llm-request golden diterjemahkan ke body `/api/chat`;
    hasil call memakai shape golden dengan nilai dari respons simulasi."""
    captured = {}
    ollama = provider.OllamaProvider(transport=_capturing_transport(captured))

    result = ollama.call(GOLDEN_LLM_REQUEST)

    assert captured["url"] == "http://127.0.0.1:11434/api/chat"
    assert captured["body"] == {
        "model": "gpt-oss:120b-cloud",
        "messages": [{"role": "user", "content": "hi"}],
        "stream": False,
        "options": {"temperature": 0.7, "num_predict": 512},
    }
    assert captured["timeout"] == 60.0
    assert set(result.keys()) == GOLDEN_LLM_RESPONSE_KEYS
    assert result["content"] == "Hello"
    assert result["model"] == "gpt-oss:120b-cloud"
    assert result["session_id"] == "session-123"
    assert result["message_json"] == '{"role": "assistant", "content": "Hello"}'
    assert result["tokens_used"] == 4
    assert result["finish_reason"] == "stop"
    assert json.loads(result["raw_response_json"])["done"] is True


def test_ollama_provider_force_json_sets_format():
    """OllamaProvider: force_json True (strict boolean) -> `format: "json"`."""
    captured = {}
    request = dict(GOLDEN_LLM_REQUEST)
    request["force_json"] = True
    provider.OllamaProvider(transport=_capturing_transport(captured)).call(request)
    assert captured["body"]["format"] == "json"


def test_ollama_provider_omits_options_when_fields_missing():
    """OllamaProvider: temperature/max_tokens None -> tidak ada kunci `options`."""
    captured = {}
    provider.OllamaProvider(transport=_capturing_transport(captured)).call(
        {"model": "m", "messages_json": "[]"}
    )
    assert "options" not in captured["body"]
    assert "format" not in captured["body"]


def test_ollama_provider_falls_back_to_provider_model():
    """OllamaProvider: request.model None -> memakai model konfigurasi provider."""
    captured = {}
    ollama = provider.OllamaProvider(
        transport=_capturing_transport(captured), model="fallback-model"
    )
    ollama.call({"messages_json": "[]"})
    assert captured["body"]["model"] == "fallback-model"


def test_ollama_provider_normalizes_base_url_trailing_slash():
    """OllamaProvider: trailing slash pada base_url dihilangkan sebelum join."""
    captured = {}
    provider.OllamaProvider(
        base_url="http://127.0.0.1:11434/", transport=_capturing_transport(captured)
    ).call({"model": "m", "messages_json": "[]"})
    assert captured["url"] == "http://127.0.0.1:11434/api/chat"


def test_ollama_provider_ignores_stream_key_in_metadata_json():
    """OllamaProvider: metadata_json bukan tempat sinyal stream — `stream`
    dalam metadata TIDAK mengubah body (stream tetap False) dan metadata
    tidak diteruskan ke Ollama (invarian U14; stream = query param U31)."""
    captured = {}
    request = dict(GOLDEN_LLM_REQUEST)
    request["metadata_json"] = '{"stream": true}'
    provider.OllamaProvider(transport=_capturing_transport(captured)).call(request)
    assert captured["body"]["stream"] is False
    assert captured["body"].get("metadata_json") is None



def test_ollama_provider_wraps_transport_http_error_in_llm_error():
    """OllamaProvider: URLError dari transport (mentah HTTP) -> LlmError."""

    def failing_transport(url, body, timeout):
        raise urllib.error.URLError("connection refused")

    ollama = provider.OllamaProvider(transport=failing_transport)
    with pytest.raises(provider.LlmError):
        ollama.call({"model": "m", "messages_json": "[]"})


def test_ollama_provider_wraps_transport_timeout_in_llm_error():
    """OllamaProvider: timeout di batas HTTP -> LlmError."""

    def timing_out_transport(url, body, timeout):
        raise TimeoutError("timed out")

    ollama = provider.OllamaProvider(transport=timing_out_transport)
    with pytest.raises(provider.LlmError):
        ollama.call({"model": "m", "messages_json": "[]"})


def test_ollama_provider_rejects_non_object_provider_response():
    """OllamaProvider: respons provider bukan JSON object -> LlmError."""
    ollama = provider.OllamaProvider(
        transport=lambda url, body, timeout: ["not", "an", "object"]
    )
    with pytest.raises(provider.LlmError):
        ollama.call({"model": "m", "messages_json": "[]"})


def test_ollama_provider_requires_model_when_request_has_none():
    """OllamaProvider: tanpa model di request maupun provider -> LlmError."""
    ollama = provider.OllamaProvider(transport=lambda url, body, timeout: {})
    with pytest.raises(provider.LlmError):
        ollama.call({"messages_json": "[]"})


def test_ollama_provider_rejects_invalid_messages_json():
    """OllamaProvider: messages_json bukan JSON valid -> ValueError (input wire)."""
    ollama = provider.OllamaProvider(transport=lambda url, body, timeout: {})
    with pytest.raises(ValueError):
        ollama.call({"model": "m", "messages_json": "{not json"})


def test_ollama_provider_rejects_non_array_messages_json():
    """OllamaProvider: messages_json bukan array JSON -> ValueError (input wire)."""
    ollama = provider.OllamaProvider(transport=lambda url, body, timeout: {})
    with pytest.raises(ValueError):
        ollama.call({"model": "m", "messages_json": '{"role": "user"}'})
