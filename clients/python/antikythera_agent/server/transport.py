"""HTTP wire transport backed by a bounded provider timeout and stdlib threads.

The transport validates browser origins and optional bearer credentials. Static
jco artifacts are public because browser ESM imports cannot set auth headers.
LLM streaming emits incremental SSE tokens; its complete HTTP response is the
source of committed content. A reconnect is a new subscription, without replay.
Use a different Transport implementation when thread-per-connection is unsuitable.
"""

from __future__ import annotations

import hmac
import json
import logging
import re
import threading
from abc import ABC, abstractmethod
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Callable, Dict, List, Optional, Tuple
from urllib.parse import parse_qs, unquote, urlsplit

from . import wire as wire_module
from .component import BASE_PATH, MANIFEST_PATH, ComponentServer
from .control import ControlChannel
from .gate import PermissionDeniedError, PolicyGate
from .provider import LlmProvider, resolve_provider
from .registry import UnionRegistry

logger = logging.getLogger(__name__)

#: Client id server-side untuk push `llm-token` streaming (mirror flag Rust
#: `--client-id`; nilai default menyamai sampel golden `llm_token_event`).
DEFAULT_CLIENT_ID = "client-a"

#: Body 404 endpoint tak dikenal / file component tak ada (pola FakeWireServer).
ERROR_NOT_FOUND = {"error": "not found"}

#: Body 400 SSE tanpa client_id (WIRE_PROTOCOL §2.4: REQUIRED).
ERROR_CLIENT_ID_REQUIRED = {"error": "client_id is required"}

#: Path POST-back: /antikythera/v1/events/{correlation-id}/response.
_POSTBACK_RE = re.compile(r"^/antikythera/v1/events/([^/]+)/response$")


def _compact_json(value: Any) -> str:
    """JSON UTF-8 compact (mirror serde_json) untuk nilai wire."""
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def _error_message(exc: BaseException) -> str:
    """Pesan error tanpa artefak repr (KeyError.args[0] = pesan mentah)."""
    if isinstance(exc, KeyError) and exc.args:
        return exc.args[0]
    return str(exc)


def _echo_step_id(body: Dict[str, Any]) -> Any:
    """Echo `step-id` request; non-number/None -> 0 (mirror JS FakeWireServer)."""
    step_id = body.get("step-id")
    if isinstance(step_id, bool) or not isinstance(step_id, (int, float)):
        return 0
    return step_id


def _lifecycle_envelope(client_id: str) -> Dict[str, Any]:
    """Envelope `lifecycle` signal `connected` — shape golden `lifecycle_event`."""
    return {
        "type": "lifecycle",
        "correlation_id": None,
        "session_id": None,
        "client_id": client_id,
        "payload": {"signal": "connected"},
    }


def _llm_token_envelope(client_id: str, session_id: Optional[str], chunk: str) -> Dict[str, Any]:
    """Envelope `llm-token` — shape golden `llm_token_event` persis."""
    return {
        "type": "llm-token",
        "correlation_id": None,
        "session_id": session_id,
        "client_id": client_id,
        "payload": {"session_id": session_id, "chunk": chunk, "correlation_id": None},
    }


class Transport(ABC):
    """Port interface wire protocol (D3): handler per endpoint + lifecycle.

    Logika wire hidup di base dan dipakai bersama oleh mekanisme server apa
    pun; hanya `serve_forever`/`stop` yang wajib diimplementasi transport
    pengganti (asyncio/aiohttp dapat memakai handler yang sama tanpa
    mengubah unit hilir). Semua handler mengembalikan
    `(status, body)` dengan body JSON-serializable; 204 -> `(204, None)`;
    file component -> `(200, bytes, mime)` atau None (404).


    GET /antikythera/v1/events  (SSE control channel).
    """

    def __init__(
        self,
        registry: Optional[UnionRegistry] = None,
        gate: Optional[PolicyGate] = None,
        control: Optional[ControlChannel] = None,
        component: Optional[ComponentServer] = None,
        provider_resolver: Optional[Callable[[Optional[str]], LlmProvider]] = None,
        client_id: str = DEFAULT_CLIENT_ID,
        auth_token: Optional[str] = None,
        allowed_origins: Optional[List[str]] = None,
    ) -> None:
        """Komposisi penuh: semua dependensi injectable (test memakai fake)."""
        if registry is None:
            registry = UnionRegistry()
        if gate is None:
            gate = PolicyGate()
        if control is None:
            control = ControlChannel()
        if component is None:
            component = ComponentServer()
        if provider_resolver is None:
            provider_resolver = resolve_provider
        if not callable(provider_resolver):
            raise TypeError("transport: provider_resolver must be callable")
        if not isinstance(client_id, str) or not client_id:
            raise ValueError("transport: client_id must be a non-empty string")
        self._registry = registry
        self._gate = gate
        self._control = control
        self._component = component
        self._provider_resolver = provider_resolver
        self._client_id = client_id
        self._auth_token = auth_token
        self._allowed_origins = allowed_origins or []


    def handle_llm_call(
        self, request_body: Any, stream: bool = False, client_id: Optional[str] = None
    ) -> Tuple[int, Dict[str, Any]]:
        """Proxy LLM: resolve provider, panggil, kembalikan llm-response golden.

        `stream` hanya dari QUERY PARAM `?stream=true` (WIRE_PROTOCOL §2.1);
        body TIDAK pernah dibaca sebagai sinyal streaming. Saat `stream=True`,
        token `llm-token` (chunk = konten utuh respons, mirror Rust stub
        `call_stream`) di-queue ke SSE channel SEBELUM respons di-resolve
        (invarian F5 — determinisme streaming JS, §4).


        Normalisasi lewat parser golden: kunci persis llm_call_response, ekstra field dari
        provider cacat ditolak keras (invarian 5).

        F5: push SINKRON sebelum handler mengembalikan respons — penulisan frame ke socket SSE
        terjadi sebelum respons HTTP llm/call di-resolve (hasil push diabaikan, mirror Rust
        `let _`).
        """
        if not isinstance(request_body, dict):
            return (400, {"error": "llm-request must be an object"})
        try:
            self._gate.check_llm(request_body.get("session_id"))
        except PermissionDeniedError as exc:
            return (403, {"error": str(exc)})
        try:
            provider = self._provider_resolver(request_body.get("provider"))
        except Exception as exc:
            return (400, {"error": _error_message(exc)})
        try:
            token_client = client_id or self._client_id

            def emit(chunk: str) -> None:
                self._control.push(
                    token_client,
                    _llm_token_envelope(token_client, request_body.get("session_id"), chunk),
                )

            response = (
                provider.call_stream(request_body, emit)
                if stream and callable(getattr(provider, "call_stream", None))
                else provider.call(request_body)
            )
        except Exception as exc:
            return (400, {"error": _error_message(exc)})
        if not isinstance(response, dict):
            return (500, {"error": "provider returned an invalid llm-response"})
        normalized = wire_module.parse_llm_response(response)
        if stream and not callable(getattr(provider, "call_stream", None)):
            session_id = request_body.get("session_id")
            content = normalized.get("content")
            chunk = content if isinstance(content, str) else ""
            self._control.push(
                self._client_id, _llm_token_envelope(self._client_id, session_id, chunk)
            )
        return (200, normalized)


    def handle_tools_execute(self, request_body: Any) -> Tuple[int, Dict[str, Any]]:
        """Eksekusi tool server/mcp di belakang gate (semantik routing.rs).

        Urutan mirror Rust `execute_server_owned`: unknown owner -> denial
        `permission:` 403; owner client -> denial 403; gate -> denial 403;
        handler hilang -> 400; arguments-json gagal di-parse -> 400; failure
        handler adalah HASIL success=false, bukan error HTTP.
        """
        if not isinstance(request_body, dict):
            return (400, {"error": "tool-call-event must be an object"})
        tool_name = request_body.get("tool-name")
        if not isinstance(tool_name, str) or not tool_name:
            return (400, {"error": "tool-call-event requires a non-empty tool-name"})
        owner = self._registry.owner_of(tool_name)
        if owner is None:
            return (
                403,
                {"error": f"permission: tool '{tool_name}' not in allowlist"},
            )
        if owner == wire_module.WIRE["OWNER_CLIENT"]:
            return (
                403,
                {
                    "error": (
                        f"permission: tool '{tool_name}' is owned by the client; "
                        f"server cannot execute it"
                    )
                },
            )
        try:
            self._gate.check_tool(owner, tool_name)
        except PermissionDeniedError as exc:
            return (403, {"error": str(exc)})
        handler = self._registry.handler_of(tool_name)
        if handler is None:
            return (400, {"error": f"tool '{tool_name}' has no server handler"})
        try:
            arguments = json.loads(request_body.get("arguments-json", "{}"))
        except (TypeError, json.JSONDecodeError) as exc:
            return (
                400,
                {"error": f"tool '{tool_name}': cannot parse arguments-json: {exc}"},
            )
        try:
            output = handler(arguments)
        except Exception as exc:
            return (
                200,
                {
                    "tool-name": tool_name,
                    "success": False,
                    "output-json": "{}",
                    "error-message": _error_message(exc),
                    "step-id": _echo_step_id(request_body),
                },
            )
        return (
            200,
            {
                "tool-name": tool_name,
                "success": True,
                "output-json": _compact_json(output),
                "error-message": None,
                "step-id": _echo_step_id(request_body),
            },
        )


    def handle_tools_list(self) -> Tuple[int, List[Dict[str, Any]]]:
        """Registry pull: array ToolDefinition shape golden (C1)."""
        return (200, self._registry.definitions())


    def handle_events(
        self,
        client_id: str,
        session_id: Optional[str],
        writer: Callable[[bytes], None],
        reader: Callable[[], Optional[bytes]],
    ) -> None:
        """Kelola koneksi SSE satu client sampai disconnect.

        `writer` menulis frame SSE `(bytes) -> None` (harus aman dipanggil
        dari beberapa thread dan di-flush per frame — kontrak control.py).
        `reader` memblokir dan mengembalikan potongan byte, None pada
        EOF/error. Urutan: register -> push lifecycle `connected` -> blokir
        sampai disconnect -> unregister (fail-closed presence, §5).
        `session_id` diteruskan untuk lifecycle scope developer (decision 8);
        lifecycle push memakai nilai None sesuai golden `lifecycle_event`.
        """
        if not isinstance(client_id, str) or not client_id.strip():
            raise ValueError("transport: events requires a non-empty client_id")
        self._control.register_client(client_id, writer)
        self._control.push(client_id, _lifecycle_envelope(client_id))
        try:
            while True:
                try:
                    data = reader()
                except (OSError, ConnectionError):
                    break
                if not data:
                    break
        finally:
            self._control.unregister_client(client_id, writer)


    def handle_postback(self, correlation_id: str, request_body: Any) -> Tuple[int, Optional[Any]]:
        """POST-back: resolve correlation; selalu 204 (unknown/mismatch
        diabaikan + di-log oleh ControlChannel — mirror Rust http.rs)."""
        if not isinstance(request_body, dict):
            return (400, {"error": "postback body must be an object"})
        self._control.resolve_postback(correlation_id, request_body)
        return (204, None)


    def handle_component_manifest(self) -> Tuple[int, Dict[str, Any]]:
        """Manifest bundle jco — shape golden `component_manifest` (D4)."""
        return (200, self._component.manifest())

    def handle_component_path(self, path: str) -> Optional[Tuple[int, bytes, str]]:
        """File bundle; None -> 404. Menerima path yang SUDAH di-URL-decode
        (kontrak U22: decode adalah tanggung jawab transport/router)."""
        resolved = self._component.resolve(path)
        if resolved is None:
            return None
        content, mime = resolved
        return (200, content, mime)


    @abstractmethod
    def serve_forever(self, host: str = "127.0.0.1", port: int = 0) -> None:
        """Jalankan server secara blocking sampai `stop` dipanggil."""

    @abstractmethod
    def stop(self) -> None:
        """Hentikan server yang sedang berjalan."""


class ThreadingHttpTransport(Transport):
    """Default transport: stdlib ThreadingHTTPServer (D3, zero-dependency).

    `start()` mengikat socket (port 0 = ephemeral) dan melayani di thread
    daemon; `serve_forever()` adalah varian blocking. Router me-URL-decode
    path SEBELUM resolve (kontrak U22) dan memetakan handler ke status/body.
    """

    def __init__(self, *args: Any, **kwargs: Any) -> None:
        super().__init__(*args, **kwargs)
        self._server: Optional[ThreadingHTTPServer] = None
        self._thread: Optional[threading.Thread] = None

    def _make_handler_class(self) -> type:
        return type("_WireRequestHandler", (_WireRequestHandler,), {"transport": self})

    def start(self, host: str = "127.0.0.1", port: int = 0) -> int:
        """Bind lalu serve di thread daemon; mengembalikan port yang terikat."""
        self._server = ThreadingHTTPServer((host, port), self._make_handler_class())
        self._thread = threading.Thread(
            target=self._server.serve_forever,
            name="antikythera-wire-http",
            daemon=True,
        )
        self._thread.start()
        return self._server.server_address[1]

    def serve_forever(self, host: str = "127.0.0.1", port: int = 0) -> None:
        self._server = ThreadingHTTPServer((host, port), self._make_handler_class())
        try:
            self._server.serve_forever()
        finally:
            self._server.server_close()
            self._server = None

    @property
    def server_address(self) -> Optional[Tuple[str, int]]:
        """Alamat terikat, atau None sebelum `start`/`serve_forever`."""
        if self._server is None:
            return None
        addr: Any = self._server.server_address
        if isinstance(addr, tuple) and len(addr) >= 2:
            return (str(addr[0]), int(addr[1]))
        return None

    def stop(self) -> None:
        """Shutdown server + tutup socket + join thread (idempotent)."""
        server, thread = self._server, self._thread
        self._server = None
        self._thread = None
        if server is not None:
            server.shutdown()
            server.server_close()
        if thread is not None:
            thread.join(timeout=5.0)

    def __enter__(self) -> "ThreadingHttpTransport":
        self.start()
        return self

    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> None:
        self.stop()


class _WireRequestHandler(BaseHTTPRequestHandler):
    """Router HTTP -> handler Transport; satu instance per koneksi (thread)."""

    transport: Optional[Transport] = None
    server_version = "AntikytheraWire/1.0"

    def log_message(self, fmt: str, *args: Any) -> None:
        logger.debug("wire http: %s", fmt % args)


    def end_headers(self) -> None:
        origin = self.headers.get("Origin")
        if self.transport and origin in self.transport._allowed_origins:
            self.send_header("Access-Control-Allow-Origin", origin)
            self.send_header("Vary", "Origin")
            self.send_header(
                "Access-Control-Allow-Headers",
                "Authorization, Content-Type, X-Antikythera-Client-Id",
            )
            self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        super().end_headers()

    def do_OPTIONS(self) -> None:
        if self.transport and self.headers.get("Origin") in self.transport._allowed_origins:
            self._send_empty(204)
        else:
            self._send_json(403, {"error": "permission: origin not allowed"})

    def do_GET(self) -> None:  # noqa: N802 (BaseHTTPRequestHandler API)
        self._dispatch()

    def do_POST(self) -> None:  # noqa: N802
        self._dispatch()

    def _dispatch(self) -> None:
        """Public static artifacts need no bearer header (ESM imports cannot set it).

        Kontrak U22: decode SEBELUM resolve.
        """
        transport = self.transport
        if transport is None:
            self._send_json(500, {"error": "transport not configured"})
            return
        origin = self.headers.get("Origin")
        if origin and origin not in transport._allowed_origins:
            self._send_json(403, {"error": "permission: origin not allowed"})
            return
        parsed = urlsplit(self.path)
        if transport._auth_token and not parsed.path.startswith(BASE_PATH):
            expected = "Bearer " + transport._auth_token
            if not hmac.compare_digest(self.headers.get("Authorization", ""), expected):
                self._send_json(401, {"error": "permission: authentication required"})
                return
        path = unquote(parsed.path)
        query = parse_qs(parsed.query, keep_blank_values=True)
        status: int
        body: Any
        try:
            if path == wire_module.WIRE["TOOLS_LIST"]:
                status, body = transport.handle_tools_list()
                self._send_json(status, body)
            elif path == wire_module.WIRE["EVENTS"]:
                self._handle_sse(query)
            elif path == MANIFEST_PATH:
                status, body = transport.handle_component_manifest()
                self._send_json(status, body)
            elif path.startswith(BASE_PATH):
                self._handle_component_file(path[len(BASE_PATH) :])
            elif self.command == "POST" and path == wire_module.WIRE["LLM_CALL"]:
                status, body = transport.handle_llm_call(
                    self._read_json_body(),
                    stream=query.get("stream") == ["true"],
                    client_id=self.headers.get("X-Antikythera-Client-Id"),
                )
                self._send_json(status, body)
            elif self.command == "POST" and path == wire_module.WIRE["TOOLS_EXECUTE"]:
                status, body = transport.handle_tools_execute(self._read_json_body())
                self._send_json(status, body)
            else:
                match = _POSTBACK_RE.match(path)
                if self.command == "POST" and match is not None:
                    status, body = transport.handle_postback(match.group(1), self._read_json_body())
                    if status == 204:
                        self._send_empty(204)
                    else:
                        self._send_json(status, body)
                else:
                    self._send_json(404, ERROR_NOT_FOUND)
        except Exception:
            logger.exception(
                "wire http: unhandled handler error for %s %s", self.command, self.path
            )
            try:
                self._send_json(500, {"error": "internal error"})
            except Exception:
                pass


    def _handle_sse(self, query: Dict[str, List[str]]) -> None:
        """Writer dikunci per koneksi + flush per frame: kontrak control.py (dipanggil dari
        keepalive/lifecycle/llm-token threads).
        """
        client_id = (query.get("client_id") or [""])[0]
        if not client_id.strip():
            self._send_json(400, ERROR_CLIENT_ID_REQUIRED)
            return
        sid_values = query.get("session_id")
        session_id: Optional[str] = sid_values[0] if sid_values else None
        transport = self.transport
        if transport is None:
            self._send_json(500, {"error": "transport not configured"})
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()
        self.wfile.flush()
        transport.handle_events(client_id, session_id, self._make_sse_writer(), self._sse_read)

    def _make_sse_writer(self) -> Callable[[bytes], None]:
        lock = threading.Lock()
        wfile = self.wfile

        def write(frame: bytes) -> None:
            with lock:
                wfile.write(frame)
                wfile.flush()

        return write

    def _sse_read(self) -> Optional[bytes]:
        """Baca satu byte dari client; None pada EOF/error (tanda disconnect)."""
        try:
            line = self.rfile.readline(1)
        except (OSError, ConnectionError):
            return None
        return line if line else None

    def _handle_component_file(self, rel_path: str) -> None:
        transport = self.transport
        if transport is None:
            self._send_json(500, {"error": "transport not configured"})
            return
        result = transport.handle_component_path(rel_path)
        if result is None:
            self._send_json(404, ERROR_NOT_FOUND)
            return
        status, content, mime = result
        self.send_response(status)
        self.send_header("Content-Type", mime)
        self.send_header("Content-Length", str(len(content)))
        self.end_headers()
        self.wfile.write(content)
        self.wfile.flush()

    def _read_json_body(self) -> Any:
        length = self.headers.get("Content-Length")
        if not length:
            return None
        try:
            raw = self.rfile.read(int(length))
        except (OSError, ValueError):
            return None
        try:
            return json.loads(raw.decode("utf-8"))
        except (json.JSONDecodeError, UnicodeDecodeError):
            return None

    def _send_json(self, status: int, body: Any) -> None:
        payload = _compact_json(body).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)
        self.wfile.flush()

    def _send_empty(self, status: int) -> None:
        self.send_response(status)
        self.send_header("Content-Length", "0")
        self.end_headers()
        self.wfile.flush()
