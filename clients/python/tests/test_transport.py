"""Falsification suite untuk `antikythera_agent.server.transport` (unit U31).

Peran Coder — verifikasi mekanis WAJIB sebelum deklarasi selesai. Suite ini
menguji kontrak sambungan U31 yang dikonsumsi unit hilir (U32 bridge.py,
U61 parity test runtime-bridge.test.mjs, U62 E2E manifest + bundle):

- Port interface (D3): handler per endpoint; ThreadingHttpTransport default
  di atas ThreadingHTTPServer stdlib (zero-dependency).
- Wire shape: respons persis golden (llm_call_response,
  tool_execute_response, tools_list_response, component_manifest,
  lifecycle_event, llm_token_event, error_event.payload) — nol field di
  luar golden (invarian 5, WIRE_PROTOCOL §7).
- Denial gate: 403 body `{"error": "permission: ..."}` persis
  `error_event.payload` (R4); owner client ditolak 403; failure handler
  adalah HASIL success=false (bukan error HTTP) — mirror routing.rs.
- SSE: `client_id` wajib (tanpa -> 400); lifecycle `connected`;
  unregister fail-closed saat disconnect.
- Streaming (invarian F5): token `llm-token` di-queue ke SSE channel
  SEBELUM respons POST /llm/call di-resolve; `?stream=true` query param,
  BUKAN di body (§2.1).
- URL-decode path component SEBELUM resolve (kontrak U22).

Sumber kebenaran:
- documentation/WIRE_PROTOCOL.md §2/§3/§4/§5/§7
- contracts/shared/wire_protocol.golden.json
- antikythera-server-runtime/src/http.rs + routing.rs + control.rs
- npm/antikythera-sdk/runtime/transport.js + sse.js
- npm/antikythera-sdk/test/runtime-bridge.test.mjs (parity client-side)

Amplop test (asumsi yang dideklarasikan agar sertifikasi tetap sah):
- Unit level memakai fake dependensi (registry/gate/control/component/
  provider) — transport TIDAK membuat instance internal yang tidak bisa
  di-inject (D3); mekanisme serve tidak diuji di unit.
- Integrasi threading memakai server nyata di port ephemeral + urllib.request
  stdlib + ControlChannel nyata (thread daemon keepalive).
- Urutan F5 dibuktikan deterministik: ObservingTransport (subclass port yang
  membungkus SSE writer) men-set Event saat frame `llm-token` ditulis ke
  socket; karena push token sinkron di jalur pemanggil POST /llm/call, Event
  PASTI ter-set saat POST selesai (memory barrier, tanpa race).
- Bundle memakai fixture tmp_path (D1: bundle package data belum tentu ada
  di source tree).

Menjalankan (dari repo root):
    $env:PYTHONPATH="python"
    python -m pytest python/tests/test_transport.py -v
"""

from __future__ import annotations

import copy
import json
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from types import SimpleNamespace
from typing import Any, Callable, Dict, List, Optional

import pytest

from antikythera_agent.server.component import ComponentServer
from antikythera_agent.server.control import ControlChannel
from antikythera_agent.server.gate import PermissionDeniedError, PolicyGate
from antikythera_agent.server.provider import LlmError, StubProvider
from antikythera_agent.server.registry import UnionRegistry
from antikythera_agent.server.transport import (
    DEFAULT_CLIENT_ID,
    ThreadingHttpTransport,
    Transport,
)

_GOLDEN_PATH = (
    Path(__file__).resolve().parents[3] / "contracts" / "shared" / "wire_protocol.golden.json"
)

GOLDEN = json.loads(_GOLDEN_PATH.read_text(encoding="utf-8"))

#: Item ToolDefinition kanonik golden — dipakai registrasi server tool nyata.
GOLDEN_TOOL_DEF = copy.deepcopy(GOLDEN["tools_list_response"][0])

JS_BYTES = b'export const sdk = "test";\n'
WASM_BYTES = b"\x00asm\x01\x00\x00\x00"


# ===========================================================================
# Fake dependensi (unit level) — transport tidak boleh butuh instance nyata
# ===========================================================================

class FakeRegistry:
    """Duck-typed UnionRegistry: definitions/owner_of/handler_of."""

    def __init__(
        self,
        definitions: Optional[List[Dict[str, Any]]] = None,
        owners: Optional[Dict[str, str]] = None,
        handlers: Optional[Dict[str, Callable[[Any], Any]]] = None,
    ) -> None:
        self._definitions = definitions if definitions is not None else []
        self._owners = dict(owners or {})
        self._handlers = dict(handlers or {})

    def definitions(self) -> List[Dict[str, Any]]:
        return self._definitions

    def owner_of(self, name: str) -> Optional[str]:
        return self._owners.get(name)

    def handler_of(self, name: str) -> Optional[Callable[[Any], Any]]:
        return self._handlers.get(name)


class FakeGate:
    """Duck-typed PolicyGate: denial terprogram via tuple (destination, name)."""

    def __init__(self, denied=()) -> None:
        self._denied = set(denied)
        self.checked: List[tuple] = []

    def check_tool(self, destination: str, name: str) -> None:
        self.checked.append((destination, name))
        if (destination, name) in self._denied:
            raise PermissionDeniedError(f"permission: tool '{name}' not in allowlist")


class FakeControl:
    """Duck-typed ControlChannel yang merekam panggilan transport."""

    def __init__(self) -> None:
        self.pushed: List[tuple] = []
        self.registered: List[str] = []
        self.unregistered: List[str] = []
        self.resolved: List[tuple] = []

    def register_client(self, client_id: str, writer: Callable[[bytes], None]) -> None:
        self.registered.append(client_id)

    def unregister_client(self, client_id: str) -> bool:
        self.unregistered.append(client_id)
        return True

    def push(self, client_id: str, envelope: Dict[str, Any]) -> bool:
        self.pushed.append((client_id, envelope))
        return True

    def resolve_postback(self, correlation_id: str, body: Dict[str, Any]) -> bool:
        self.resolved.append((correlation_id, body))
        return True


class FakeComponent:
    """Duck-typed ComponentServer: manifest tetap + peta file; merekam resolve."""

    def __init__(
        self,
        manifest: Optional[Dict[str, Any]] = None,
        files: Optional[Dict[str, tuple]] = None,
    ) -> None:
        self._manifest = manifest if manifest is not None else GOLDEN["component_manifest"]
        self._files = files if files is not None else {}
        self.resolve_calls: List[str] = []

    def manifest(self) -> Dict[str, Any]:
        return self._manifest

    def resolve(self, path: str) -> Optional[tuple]:
        self.resolve_calls.append(path)
        return self._files.get(path)


class FakeProvider:
    """Duck-typed LlmProvider dengan respons/error terprogram."""

    def __init__(self, response: Any = None, error: Optional[Exception] = None) -> None:
        self._response = GOLDEN["llm_call_response"] if response is None else response
        self._error = error
        self.calls: List[Dict[str, Any]] = []

    def call(self, request: Dict[str, Any]) -> Dict[str, Any]:
        self.calls.append(request)
        if self._error is not None:
            raise self._error
        return self._response


class _UnitTransport(Transport):
    """Subclass konkret port untuk unit test handler (mekanisme tak diuji)."""

    def serve_forever(self, host: str = "127.0.0.1", port: int = 0) -> None:
        raise NotImplementedError("unit transport has no serve loop")

    def stop(self) -> None:
        raise NotImplementedError("unit transport has no server")


class _RecordingWriter:
    """Writer `(bytes) -> None` perekam frame (pola test_control.py)."""

    def __init__(self) -> None:
        self.frames: List[bytes] = []

    def __call__(self, data: bytes) -> None:
        self.frames.append(data)


def _make_unit_transport(**overrides: Any) -> _UnitTransport:
    """Transport unit dengan fake dependensi; overrides per test."""
    defaults: Dict[str, Any] = dict(
        registry=FakeRegistry(),
        gate=FakeGate(),
        control=FakeControl(),
        component=FakeComponent(),
        provider_resolver=lambda name: FakeProvider(),
        client_id=DEFAULT_CLIENT_ID,
    )
    defaults.update(overrides)
    return _UnitTransport(**defaults)


def _wait_for(predicate, timeout_secs=5.0, step=0.01):
    """Polling deterministik sampai predicate True atau timeout habis."""
    deadline = time.monotonic() + timeout_secs
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(step)
    return predicate()


# ===========================================================================
# Unit: POST /antikythera/v1/llm/call
# ===========================================================================

def test_llm_call_returns_golden_response_without_stream():
    """llm/call non-stream: respons persis golden; TIDAK ada push token."""
    control = FakeControl()
    transport = _make_unit_transport(
        control=control,
        provider_resolver=lambda name: FakeProvider(),
    )
    status, body = transport.handle_llm_call(GOLDEN["llm_call_request"], stream=False)
    assert status == 200
    assert body == GOLDEN["llm_call_response"]
    assert control.pushed == []


def test_llm_call_stream_pushes_golden_token_before_resolve():
    """F5 (unit): stream=true meng-queue `llm-token` persis golden SEBELUM
    handler mengembalikan respons (push sinkron di jalur pemanggil)."""
    control = FakeControl()
    provider = FakeProvider()
    transport = _make_unit_transport(
        control=control,
        provider_resolver=lambda name: provider,
    )
    status, body = transport.handle_llm_call(GOLDEN["llm_call_request"], stream=True)
    assert status == 200
    assert body == GOLDEN["llm_call_response"]
    assert len(control.pushed) == 1
    client_id, envelope = control.pushed[0]
    assert client_id == DEFAULT_CLIENT_ID
    assert envelope == GOLDEN["llm_token_event"]


def test_llm_call_stream_chunk_is_whole_content():
    """Token chunk = konten utuh respons (mirror Rust stub `call_stream`)."""
    control = FakeControl()
    response = dict(GOLDEN["llm_call_response"])
    response["content"] = "multi-word content"
    transport = _make_unit_transport(
        control=control,
        provider_resolver=lambda name: FakeProvider(response=response),
    )
    transport.handle_llm_call(GOLDEN["llm_call_request"], stream=True)
    envelope = control.pushed[0][1]
    assert envelope["payload"]["chunk"] == "multi-word content"


def test_llm_call_stream_flag_not_read_from_body():
    """§2.1: streaming disinyalkan via query param, BUKAN field body."""
    control = FakeControl()
    transport = _make_unit_transport(control=control)
    body = dict(GOLDEN["llm_call_request"])
    body["stream"] = True
    status, _ = transport.handle_llm_call(body, stream=False)
    assert status == 200
    assert control.pushed == []


def test_llm_call_unknown_provider_returns_400():
    """Resolver gagal (KeyError resolve_provider) -> 400 dengan pesan mentah."""
    def resolver(name):
        raise KeyError("unknown LLM provider: 'nope'")

    transport = _make_unit_transport(provider_resolver=resolver)
    status, body = transport.handle_llm_call({"provider": "nope"})
    assert status == 400
    assert body == {"error": "unknown LLM provider: 'nope'"}


def test_llm_call_provider_failure_returns_400():
    """Kegagalan provider (LlmError) -> 400, bukan error HTTP mentah."""
    transport = _make_unit_transport(
        provider_resolver=lambda name: FakeProvider(error=LlmError("ollama request failed: boom"))
    )
    status, body = transport.handle_llm_call(GOLDEN["llm_call_request"])
    assert status == 400
    assert body == {"error": "ollama request failed: boom"}


def test_llm_call_rejects_non_object_body():
    """Entry-point guard: body non-object -> 400."""
    transport = _make_unit_transport()
    status, body = transport.handle_llm_call("not-an-object")
    assert status == 400
    assert body == {"error": "llm-request must be an object"}


def test_llm_call_provider_non_object_response_returns_500():
    """Provider melanggar kontrak U14 (bukan objek) -> 500 eksplisit."""
    transport = _make_unit_transport(
        provider_resolver=lambda name: FakeProvider(response="not-an-object")
    )
    status, body = transport.handle_llm_call(GOLDEN["llm_call_request"])
    assert status == 500
    assert body == {"error": "provider returned an invalid llm-response"}


def test_llm_call_normalizes_response_to_golden_keys():
    """Invarian 5: ekstra field dari provider dibuang keras (parser golden)."""
    response = {"content": "Hello", "extra_field": "leak"}
    transport = _make_unit_transport(
        provider_resolver=lambda name: FakeProvider(response=response)
    )
    status, body = transport.handle_llm_call(GOLDEN["llm_call_request"])
    assert status == 200
    assert set(body.keys()) == set(GOLDEN["llm_call_response"].keys())
    assert "extra_field" not in body


# ===========================================================================
# Unit: POST /antikythera/v1/tools/execute (semantik routing.rs)
# ===========================================================================

def test_tools_execute_denies_unregistered_tool_with_permission_403():
    """Unknown owner = denial `permission:` 403 (R4) — shape error_event.payload."""
    transport = _make_unit_transport(registry=FakeRegistry())
    status, body = transport.handle_tools_execute(GOLDEN["tool_execute_request"])
    assert status == 403
    assert body == {"error": "permission: tool 'get_current_time' not in allowlist"}
    assert set(body.keys()) == set(GOLDEN["error_event"]["payload"].keys())


def test_tools_execute_denies_client_owned_tool():
    """Owner client: server tidak boleh mengeksekusi — 403 `permission:`."""
    registry = FakeRegistry(owners={"client_tool": "client"})
    transport = _make_unit_transport(registry=registry)
    status, body = transport.handle_tools_execute(
        {"tool-name": "client_tool", "arguments-json": "{}", "session-id": "s", "step-id": 1}
    )
    assert status == 403
    assert body["error"].startswith("permission:")
    assert "owned by the client" in body["error"]


def test_tools_execute_gate_denial_returns_403_permission():
    """Gate ditanyakan SEBELUM eksekusi; denial -> 403 `permission:`."""
    registry = FakeRegistry(
        owners={"rm": "server"}, handlers={"rm": lambda args: "never-called"}
    )
    gate = FakeGate(denied={("server", "rm")})
    transport = _make_unit_transport(registry=registry, gate=gate)
    status, body = transport.handle_tools_execute(
        {"tool-name": "rm", "arguments-json": "{}", "session-id": "s", "step-id": 1}
    )
    assert status == 403
    assert body == {"error": "permission: tool 'rm' not in allowlist"}
    assert gate.checked == [("server", "rm")]


def test_tools_execute_success_returns_golden_result():
    """Handler sukses -> tool-execution-result persis golden."""
    registry = FakeRegistry(
        owners={"get_current_time": "server"},
        handlers={"get_current_time": lambda args: {"datetime": "2026-08-12T00:00:00Z"}},
    )
    transport = _make_unit_transport(registry=registry, gate=FakeGate())
    status, body = transport.handle_tools_execute(GOLDEN["tool_execute_request"])
    assert status == 200
    assert body == GOLDEN["tool_execute_response"]


def test_tools_execute_handler_failure_is_result_not_error():
    """Failure handler = HASIL success=false (bukan error HTTP) — routing.rs."""
    def boom(args):
        raise RuntimeError("handler exploded")

    registry = FakeRegistry(owners={"t": "server"}, handlers={"t": boom})
    transport = _make_unit_transport(registry=registry, gate=FakeGate())
    status, body = transport.handle_tools_execute(
        {"tool-name": "t", "arguments-json": "{}", "session-id": "s", "step-id": 7}
    )
    assert status == 200
    assert body == {
        "tool-name": "t",
        "success": False,
        "output-json": "{}",
        "error-message": "handler exploded",
        "step-id": 7,
    }


def test_tools_execute_missing_handler_returns_400():
    """Owner server/mcp tanpa handler -> 400 non-permission (mirror routing.rs)."""
    registry = FakeRegistry(owners={"mcp_tool": "mcp"})
    transport = _make_unit_transport(registry=registry, gate=FakeGate())
    status, body = transport.handle_tools_execute(
        {"tool-name": "mcp_tool", "arguments-json": "{}", "session-id": "s", "step-id": 1}
    )
    assert status == 400
    assert "no server handler" in body["error"]


def test_tools_execute_bad_arguments_json_returns_400_without_calling_handler():
    """arguments-json tidak valid -> 400; handler TIDAK dipanggil."""
    called = []

    def handler(args):
        called.append(args)
        return {"ok": True}

    registry = FakeRegistry(owners={"t": "server"}, handlers={"t": handler})
    transport = _make_unit_transport(registry=registry, gate=FakeGate())
    status, body = transport.handle_tools_execute(
        {"tool-name": "t", "arguments-json": "{invalid", "session-id": "s", "step-id": 1}
    )
    assert status == 400
    assert "cannot parse arguments-json" in body["error"]
    assert called == []


def test_tools_execute_rejects_non_object_body():
    """Entry-point guard: body non-object -> 400."""
    transport = _make_unit_transport()
    status, _ = transport.handle_tools_execute(["not", "object"])
    assert status == 400


def test_tools_execute_rejects_missing_tool_name():
    """tool-name wajib non-empty (serde Rust `tool_name: String`)."""
    transport = _make_unit_transport()
    status, body = transport.handle_tools_execute({"arguments-json": "{}"})
    assert status == 400
    assert "tool-name" in body["error"]


# ===========================================================================
# Unit: GET /tools, events SSE, postback, component
# ===========================================================================

def test_tools_list_returns_registry_definitions():
    """GET /tools: array ToolDefinition persis golden."""
    registry = FakeRegistry(definitions=GOLDEN["tools_list_response"])
    transport = _make_unit_transport(registry=registry)
    status, body = transport.handle_tools_list()
    assert status == 200
    assert body == GOLDEN["tools_list_response"]


def test_events_registers_pushes_lifecycle_and_unregisters_on_eof():
    """SSE: register -> lifecycle `connected` golden -> blokir -> unregister."""
    control = FakeControl()
    writer = _RecordingWriter()
    transport = _make_unit_transport(control=control)
    calls = {"n": 0}

    def reader():
        calls["n"] += 1
        return b"" if calls["n"] > 1 else b"ping"

    transport.handle_events("client-a", "session-1", writer, reader)
    assert control.registered == ["client-a"]
    assert control.unregistered == ["client-a"]
    assert len(control.pushed) == 1
    assert control.pushed[0][0] == "client-a"
    assert control.pushed[0][1] == GOLDEN["lifecycle_event"]


def test_events_reader_error_is_treated_as_disconnect():
    """Reader gagal (koneksi mati) -> unregister fail-closed (presence §5)."""
    control = FakeControl()
    transport = _make_unit_transport(control=control)

    def reader():
        raise OSError("connection reset")

    transport.handle_events("client-a", None, _RecordingWriter(), reader)
    assert control.registered == ["client-a"]
    assert control.unregistered == ["client-a"]


def test_events_rejects_empty_client_id():
    """client_id kosong ditolak di entry point (mirror Rust trim check)."""
    transport = _make_unit_transport()
    with pytest.raises(ValueError):
        transport.handle_events("", None, _RecordingWriter(), lambda: b"")


def test_postback_resolves_and_returns_204():
    """POST-back: resolve_postback dipanggil dengan path + body; 204."""
    control = FakeControl()
    transport = _make_unit_transport(control=control)
    body = {
        "correlation_id": "corr-0001",
        "ok": True,
        "payload": {"tool-name": "get_current_time"},
        "error": None,
    }
    status, resp = transport.handle_postback("corr-0001", body)
    assert (status, resp) == (204, None)
    assert control.resolved == [("corr-0001", body)]


def test_postback_returns_204_even_when_control_reports_unknown():
    """Unknown/expired correlation tetap 204; logging milik ControlChannel."""
    class UnknownControl(FakeControl):
        def resolve_postback(self, correlation_id, body):
            self.resolved.append((correlation_id, body))
            return False

    control = UnknownControl()
    transport = _make_unit_transport(control=control)
    status, _ = transport.handle_postback("never-created", {"correlation_id": "never-created"})
    assert status == 204


def test_postback_rejects_non_object_body():
    """Body POST-back non-object -> 400 (serde `Json<PostbackBody>` rejection)."""
    transport = _make_unit_transport()
    status, body = transport.handle_postback("corr-1", "not-an-object")
    assert status == 400
    assert body == {"error": "postback body must be an object"}


def test_component_manifest_returns_golden():
    """Manifest component persis golden (D4)."""
    transport = _make_unit_transport()
    status, body = transport.handle_component_manifest()
    assert status == 200
    assert body == GOLDEN["component_manifest"]


def test_component_path_returns_bytes_and_mime():
    """File component: bytes + MIME terdaftar; path diterima sudah di-decode."""
    component = FakeComponent(files={"antikythera-sdk.js": (JS_BYTES, "text/javascript")})
    transport = _make_unit_transport(component=component)
    result = transport.handle_component_path("antikythera-sdk.js")
    assert result == (200, JS_BYTES, "text/javascript")
    assert component.resolve_calls == ["antikythera-sdk.js"]


def test_component_path_missing_returns_none():
    """File tak ada -> None (transport memutuskan 404)."""
    transport = _make_unit_transport(component=FakeComponent())
    assert transport.handle_component_path("missing.js") is None


# ===========================================================================
# Unit: constructor guard
# ===========================================================================

def test_constructor_rejects_empty_client_id():
    with pytest.raises(ValueError):
        _make_unit_transport(client_id="")


def test_constructor_rejects_non_callable_provider_resolver():
    with pytest.raises(TypeError):
        _make_unit_transport(provider_resolver="not-callable")


def test_constructor_injects_all_dependencies():
    """D3: semua dependensi injectable — instance yang diberikan DIPAKAI."""
    registry = FakeRegistry()
    gate = FakeGate()
    control = FakeControl()
    component = FakeComponent()
    transport = _make_unit_transport(
        registry=registry, gate=gate, control=control, component=component
    )
    assert transport._registry is registry
    assert transport._gate is gate
    assert transport._control is control
    assert transport._component is component


# ===========================================================================
# Integrasi threading: server nyata + urllib.request (stdlib)
# ===========================================================================

@pytest.fixture
def bundle_dir(tmp_path):
    d = tmp_path / "component"
    d.mkdir()
    (d / "antikythera-sdk.js").write_bytes(JS_BYTES)
    (d / "antikythera-sdk.runner.core.wasm").write_bytes(WASM_BYTES)
    (d / "my file.js").write_bytes(b"space-file")
    return d


@pytest.fixture
def deps(bundle_dir):
    """Dependensi nyata (bukan fake): registry + gate + control + component."""
    registry = UnionRegistry()
    registry.register_server(
        GOLDEN_TOOL_DEF,
        handler=lambda args: {"datetime": "2026-08-12T00:00:00Z"},
    )
    gate = PolicyGate()
    gate.allow_tool("server", "get_current_time")
    control = ControlChannel()
    component = ComponentServer(bundle_dir=bundle_dir)
    stub = StubProvider(json.dumps(GOLDEN["llm_call_response"]))
    return SimpleNamespace(
        registry=registry,
        gate=gate,
        control=control,
        component=component,
        provider_resolver=lambda name: stub,
        client_id=DEFAULT_CLIENT_ID,
    )


@pytest.fixture
def server(deps):
    transport = ThreadingHttpTransport(
        registry=deps.registry,
        gate=deps.gate,
        control=deps.control,
        component=deps.component,
        provider_resolver=deps.provider_resolver,
        client_id=deps.client_id,
    )
    port = transport.start()
    base_url = f"http://127.0.0.1:{port}"
    yield transport, base_url
    transport.stop()


def _request(method, url, body=None):
    """Request HTTP nyata via stdlib; mengembalikan (status, raw, content-type)."""
    data = None
    headers = {}
    if body is not None:
        data = json.dumps(body).encode("utf-8")
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        resp = urllib.request.urlopen(req, timeout=10)
    except urllib.error.HTTPError as exc:
        resp = exc
    try:
        status = getattr(resp, "status", getattr(resp, "code", None))
        raw = resp.read()
        ctype = resp.headers.get("Content-Type")
    finally:
        resp.close()
    return status, raw, ctype


def _json(raw) -> Any:
    return json.loads(raw.decode("utf-8"))


def _sse_reader(url, frames, stop_event):
    """Baca frame SSE `data: ...` sampai stop; per-frame di dalam \n\n."""
    resp = urllib.request.urlopen(url, timeout=0.5)
    buffer = b""
    try:
        while not stop_event.is_set():
            try:
                line = resp.readline()
            except (OSError, ValueError):
                break
            if not line:
                break
            buffer += line
            if buffer.endswith(b"\n\n"):
                frames.append(buffer)
                buffer = b""
    finally:
        resp.close()


def _frame_envelopes(frames) -> List[Dict[str, Any]]:
    """Envelope dari frame `data: {json}`; frame keepalive `:` di-skip."""
    out = []
    for frame in frames:
        for line in frame.decode("utf-8").split("\n"):
            if line.startswith("data: "):
                out.append(json.loads(line[len("data: "):]))
    return out


def _frame_types(frames) -> List[str]:
    return [env["type"] for env in _frame_envelopes(frames)]


def test_integration_llm_call_returns_golden_response(server):
    """POST /llm/call nyata -> llm-response shape golden (content = SELURUH
    string stub, mirror Rust StubLlmProvider; PARITY U61)."""
    _, base_url = server
    status, raw, _ = _request("POST", base_url + "/antikythera/v1/llm/call", GOLDEN["llm_call_request"])
    assert status == 200
    parsed = _json(raw)
    assert set(parsed.keys()) == set(GOLDEN["llm_call_response"].keys())
    # StubProvider default fixture (`json.dumps(GOLDEN["llm_call_response"])`):
    # content = seluruh string sampel golden, model/session_id dari request.
    assert parsed["content"] == json.dumps(GOLDEN["llm_call_response"])
    assert parsed["model"] == GOLDEN["llm_call_request"]["model"]
    assert parsed["session_id"] == GOLDEN["llm_call_request"]["session_id"]
    assert parsed["finish_reason"] == "stop"


def test_integration_tools_execute_success_returns_golden_shape(server):
    """POST /tools/execute tool di-allowlist -> tool-execution-result golden."""
    _, base_url = server
    status, raw, _ = _request("POST", base_url + "/antikythera/v1/tools/execute", GOLDEN["tool_execute_request"])
    assert status == 200
    assert _json(raw) == GOLDEN["tool_execute_response"]


def test_integration_tools_execute_denial_returns_403_permission(server):
    """Denial gate -> HTTP 403 body `{"error": "permission: ..."}` persis
    error_event.payload golden (U61 parity shape)."""
    _, base_url = server
    body = {"tool-name": "rm", "arguments-json": "{}", "session-id": "session-123", "step-id": 1}
    status, raw, _ = _request("POST", base_url + "/antikythera/v1/tools/execute", body)
    assert status == 403
    parsed = _json(raw)
    assert parsed == GOLDEN["error_event"]["payload"]
    assert set(parsed.keys()) == set(GOLDEN["error_event"]["payload"].keys())


def test_integration_tools_execute_client_owned_denied(server, deps):
    """Tool owner client ditolak server dengan 403 `permission:`."""
    deps.registry.register_client({"name": "client_tool", "description": "client side"})
    _, base_url = server
    body = {"tool-name": "client_tool", "arguments-json": "{}", "session-id": "s", "step-id": 1}
    status, raw, _ = _request("POST", base_url + "/antikythera/v1/tools/execute", body)
    assert status == 403
    assert _json(raw)["error"].startswith("permission:")
    assert "owned by the client" in _json(raw)["error"]


def test_integration_tools_list_returns_golden_definitions(server):
    """GET /tools -> array ToolDefinition persis golden (C1, U61)."""
    _, base_url = server
    status, raw, _ = _request("GET", base_url + "/antikythera/v1/tools")
    assert status == 200
    assert _json(raw) == GOLDEN["tools_list_response"]


def test_integration_events_without_client_id_returns_400(server):
    """SSE tanpa client_id -> 400 (WIRE_PROTOCOL §2.4 REQUIRED)."""
    _, base_url = server
    status, raw, _ = _request("GET", base_url + "/antikythera/v1/events")
    assert status == 400
    assert _json(raw) == {"error": "client_id is required"}


def test_integration_component_manifest_returns_golden(server):
    """GET component/manifest -> persis golden (D4, U62)."""
    _, base_url = server
    status, raw, ctype = _request("GET", base_url + "/antikythera/v1/component/manifest")
    assert status == 200
    assert ctype == "application/json"
    assert _json(raw) == GOLDEN["component_manifest"]


def test_integration_component_file_served_verbatim_with_mime(server):
    """GET component/{path} -> bytes as-is + MIME `.js` (D4, U62)."""
    _, base_url = server
    status, raw, ctype = _request("GET", base_url + "/antikythera/v1/component/antikythera-sdk.js")
    assert status == 200
    assert raw == JS_BYTES
    assert ctype == "text/javascript"


def test_integration_component_path_url_decoded_before_resolve(server):
    """Kontrak U22: transport me-URL-decode path SEBELUM resolve (file spasi)."""
    _, base_url = server
    status, raw, _ = _request("GET", base_url + "/antikythera/v1/component/my%20file.js")
    assert status == 200
    assert raw == b"space-file"


def test_integration_component_missing_returns_404(server):
    """File tak ada -> 404."""
    _, base_url = server
    status, raw, _ = _request("GET", base_url + "/antikythera/v1/component/missing.js")
    assert status == 404
    assert _json(raw) == {"error": "not found"}


def test_integration_unknown_endpoint_returns_404(server):
    """Path di luar wire -> 404."""
    _, base_url = server
    status, raw, _ = _request("GET", base_url + "/antikythera/v1/nope")
    assert status == 404


def test_integration_events_sse_receives_lifecycle_connected(server, deps):
    """SSE nyata: lifecycle `connected` golden tiba; client ter-register."""
    _, base_url = server
    frames = []
    stop = threading.Event()
    url = f"{base_url}/antikythera/v1/events?client_id={deps.client_id}"
    thread = threading.Thread(target=_sse_reader, args=(url, frames, stop), daemon=True)
    thread.start()
    try:
        assert _wait_for(lambda: "lifecycle" in _frame_types(frames))
        assert _frame_envelopes(frames)[0] == GOLDEN["lifecycle_event"]
        assert deps.control.is_client_connected(deps.client_id) is True
    finally:
        stop.set()
        thread.join(timeout=2.0)
        assert _wait_for(lambda: not deps.control.is_client_connected(deps.client_id))


def test_integration_postback_resolves_pending_and_returns_204(server, deps):
    """POST-back nyata: correlation pending di-resolve via wire; 204; unknown
    tetap 204 (WIRE_PROTOCOL §5).

    Semantik U21: resolve MENANDAI entri; waiter (await_postback) yang
    MENGONSUMSI dan menghapusnya (nilai dipegang entri sampai diambil —
    mirror Rust oneshot). Wire-level diverifikasi dengan await di thread
    terpisah (pola test_control.py): body hasil normalisasi tiba.
    """
    _, base_url = server
    corr = deps.control.create_correlation(deps.client_id, ttl_secs=60)
    body = {"correlation_id": corr, "ok": True, "payload": {"done": True}, "error": None}
    status, raw, _ = _request(
        "POST", f"{base_url}/antikythera/v1/events/{corr}/response", body
    )
    assert status == 204
    assert raw == b""

    results = {}

    def await_body():
        results["body"] = deps.control.await_postback(corr, 5.0)

    thread = threading.Thread(target=await_body, daemon=True)
    thread.start()
    thread.join(timeout=2.0)
    assert not thread.is_alive()
    assert results["body"]["ok"] is True
    assert results["body"]["payload"] == {"done": True}
    assert deps.control.pending_len() == 0

    status, raw, _ = _request(
        "POST",
        f"{base_url}/antikythera/v1/events/unknown-corr/response",
        {"correlation_id": "unknown-corr", "ok": True, "payload": {}, "error": None},
    )
    assert status == 204
    assert deps.control.pending_len() == 0


# ===========================================================================
# Streaming (F5): token di SSE channel SEBELUM respons llm/call di-resolve
# ===========================================================================

class ObservingTransport(ThreadingHttpTransport):
    """Bukti deterministik F5: observer writer SSE.

    Membungkus writer SSE: saat frame `llm-token` ditulis ke socket,
    `token_written` di-set SEBELUM penulisan selesai. Karena push token
    sinkron di jalur pemanggil POST /llm/call (sebelum handler meresolve),
    `token_written.is_set()` setelah POST selesai adalah bukti penulisan
    token SEBELUM resolusi respons — tanpa race (Event = memory barrier).
    """

    def __init__(self, *args: Any, **kwargs: Any) -> None:
        super().__init__(*args, **kwargs)
        self.token_written = threading.Event()

    def handle_events(self, client_id, session_id, writer, reader):
        def observing(frame: bytes) -> None:
            if b"llm-token" in frame:
                self.token_written.set()
            writer(frame)

        return super().handle_events(client_id, session_id, observing, reader)


def test_integration_stream_token_written_before_llm_resolve(deps):
    """F5 integrasi: koneksi SSE nyata menerima `llm-token` golden SEBELUM
    respons POST /llm/call?stream=true selesai (urutan token -> resolve)."""
    transport = ObservingTransport(
        registry=deps.registry,
        gate=deps.gate,
        control=deps.control,
        component=deps.component,
        provider_resolver=deps.provider_resolver,
        client_id=deps.client_id,
    )
    port = transport.start()
    base_url = f"http://127.0.0.1:{port}"
    frames = []
    stop = threading.Event()
    thread = threading.Thread(
        target=_sse_reader,
        args=(f"{base_url}/antikythera/v1/events?client_id={deps.client_id}", frames, stop),
        daemon=True,
    )
    thread.start()
    try:
        assert _wait_for(lambda: "lifecycle" in _frame_types(frames))

        # POST stream: handler meng-queue token SSE lalu meresolve respons.
        status, raw, _ = _request(
            "POST", f"{base_url}/antikythera/v1/llm/call?stream=true",
            GOLDEN["llm_call_request"],
        )
        assert status == 200
        parsed = _json(raw)
        assert set(parsed.keys()) == set(GOLDEN["llm_call_response"].keys())
        assert parsed["content"] == json.dumps(GOLDEN["llm_call_response"])
        assert parsed["model"] == GOLDEN["llm_call_request"]["model"]
        assert parsed["session_id"] == GOLDEN["llm_call_request"]["session_id"]
        assert parsed["finish_reason"] == "stop"

        # F5: frame llm-token SUDAH ditulis ke socket SSE saat POST selesai.
        assert transport.token_written.is_set(), (
            "llm-token must be written to the SSE channel before the POST "
            "/llm/call response resolves (F5 ordering)"
        )
        assert _wait_for(lambda: "llm-token" in _frame_types(frames))
        token = next(env for env in _frame_envelopes(frames) if env["type"] == "llm-token")
        # Envelope shape golden; payload.chunk = content (seluruh string stub).
        assert set(token.keys()) == set(GOLDEN["llm_token_event"].keys())
        assert token["payload"]["chunk"] == json.dumps(GOLDEN["llm_call_response"])
        assert token["payload"]["session_id"] == GOLDEN["llm_call_request"]["session_id"]
    finally:
        stop.set()
        thread.join(timeout=2.0)
        transport.stop()
