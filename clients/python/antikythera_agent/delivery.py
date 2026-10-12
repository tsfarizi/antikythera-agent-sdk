"""Execute configured Python agents or deliver their environment as a WASM/jco client bundle."""

from __future__ import annotations

import copy
import hashlib
import json
import mimetypes
import os
import shutil
import socket
import tempfile
import threading
import time
import uuid
from dataclasses import asdict
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Dict, Optional
from urllib.parse import unquote, urlsplit

from antikythera_agent.agent import Agent
from antikythera_agent.server.registry import UnionRegistry
from antikythera_agent.tools.helpers import extract_tool_entry
from antikythera_agent.utils import get_version

_PACKAGE = Path(__file__).parent


class _BoundTool:
    """Adapt a captured definition/handler pair to Agent's existing Tool registration contract."""

    __antikythera_tool__ = True
    __tool_owner__ = "server"

    def __init__(self, entry: Dict[str, Any]) -> None:
        self._entry = entry

    def __tool_definition__(self) -> Dict[str, Any]:
        """Return the normalized definition captured when the deployment was prepared."""
        return copy.deepcopy(self._entry["definition"])

    def __tool_handler__(self) -> Any:
        """Retain the server callback without serializing or re-decorating the original tool."""
        return self._entry["handler"]


def _module_path(value: str) -> str:
    """Accept a portable module path inside the explicitly supplied binding directory."""
    if (
        not isinstance(value, str)
        or not value.endswith((".js", ".mjs"))
        or "\\" in value
        or ":" in value
        or any(part in ("", ".", "..") for part in value.split("/"))
    ):
        raise ValueError("client binding must be a relative .js or .mjs path inside bindings_dir")
    return value


def _server_url(value: Optional[str]) -> Optional[str]:
    """Validate a public host URL without copying authentication credentials into an export."""
    if value is None:
        return None
    parsed = urlsplit(value)
    _ = parsed.port
    if (
        parsed.scheme not in ("http", "https")
        or not parsed.hostname
        or parsed.username
        or parsed.password
        or parsed.query
        or parsed.fragment
    ):
        raise ValueError(
            "server_url must be an HTTP base URL without credentials, query, or fragment"
        )
    return value.rstrip("/")


def _copy_directory(source: Path, destination: Path) -> None:
    """Copy a selected environment without following links to files outside that directory."""
    destination.mkdir(parents=True, exist_ok=True)
    for entry in sorted(source.iterdir()):
        if entry.is_symlink():
            raise ValueError(f"delivery bundles cannot include symbolic links: {entry}")
        target = destination / entry.name
        if entry.is_dir():
            _copy_directory(entry, target)
        elif entry.is_file():
            shutil.copyfile(entry, target)
        else:
            raise ValueError(f"delivery bundles require regular files: {entry}")


def _install_export(source: Path, target: Path) -> None:
    """Retry transient Windows sharing failures without exposing a partially copied export."""
    for attempt in range(6):
        try:
            source.rename(target)
            return
        except PermissionError as exc:
            if os.name != "nt" or exc.winerror not in (5, 32, 33) or attempt == 5:
                raise
            time.sleep(0.04 * 2**attempt)


class AgentDeployment:
    """Snapshot an Agent's configuration and bindings for server execution and client delivery.

    Python callbacks stay in the host. Exported clients call the provider and
    permitted server tools through the delivery host unless explicit JavaScript
    bindings or runtime callbacks supply those capabilities locally.
    """

    def __init__(self, agent: Agent, *, agent_id: str) -> None:
        if not isinstance(agent, Agent):
            raise TypeError("agent must be an Agent")
        if not isinstance(agent_id, str) or not agent_id.strip():
            raise ValueError("agent_id must be a non-empty string")
        self._config = agent.get_config()
        self._resolver = agent._provider_resolver
        self._hooks = getattr(agent, "_hooks_provider", None)
        self._tools = list(agent._pending_tools)
        self._run_lock = threading.RLock()
        registry = UnionRegistry()
        self._entries: Dict[str, Dict[str, Any]] = {}
        for obj, allow in self._tools:
            entry = extract_tool_entry(obj)
            name = entry["definition"]["name"]
            if name in self._entries:
                raise ValueError(f"duplicate deployment tool '{name}'")
            if entry["owner"] != "server" or not callable(entry["handler"]):
                raise ValueError(f"tool '{name}' requires a server handler for deployment")
            registry.register_server(entry["definition"], entry["handler"])
            self._entries[name] = {**entry, "allow": allow}
        for definition in registry.definitions():
            self._entries[definition["name"]]["definition"] = definition
        self._definition: Dict[str, Any] = {
            "schemaVersion": 1,
            "sdkVersion": get_version(),
            "serverUrl": None,
            "agent": {
                "id": agent_id,
                "provider": self._config.provider,
                "model": self._config.model,
                "systemPrompt": self._config.system_prompt,
                "maxSteps": self._config.max_steps,
                "timeoutMs": self._config.timeout,
                "hooksModule": None,
                "tools": [
                    {
                        "definition": entry["definition"],
                        "allow": entry["allow"],
                        "execution": "server",
                        "module": None,
                    }
                    for entry in self._entries.values()
                ],
            },
        }
        self._agent = self.create_agent()

    @property
    def definition(self) -> Dict[str, Any]:
        """Return a detached copy of the public deployment configuration."""
        return copy.deepcopy(self._definition)

    @classmethod
    def from_agent(cls, agent: Agent, *, agent_id: str) -> AgentDeployment:
        """Capture an existing Agent without repeating its configuration."""
        return cls(agent, agent_id=agent_id)

    def create_agent(self) -> Agent:
        """Create a native Component Model session with the captured host environment."""
        agent = Agent(
            provider=self._config.provider,
            model=self._config.model,
            system_prompt=self._config.system_prompt,
            max_steps=self._config.max_steps,
            timeout=self._config.timeout,
            session_id=f"delivery-{uuid.uuid4().hex}",
            provider_resolver=self._resolver,
            hooks_provider=self._hooks,
        )
        for entry in self._entries.values():
            agent.register_tool(_BoundTool(entry), allow=entry["allow"])
        return agent

    def run(self, prompt: str) -> Any:
        """Execute consecutive turns in a deployment-owned server session with serialized access."""
        with self._run_lock:
            return self._agent.run(prompt)

    def call_provider(self, request: Dict[str, Any]) -> Dict[str, Any]:
        """Call the provider and model pinned by this deployment."""
        from antikythera_agent.server.provider import resolve_provider

        resolver = self._resolver or resolve_provider
        if not isinstance(request.get("messages_json"), str) or not isinstance(
            json.loads(request["messages_json"]), list
        ):
            raise ValueError("messages_json must encode an array")
        provider = copy.copy(resolver(self._config.provider))
        if hasattr(provider, "timeout"):
            provider.timeout = self._config.timeout / 1000
        response = provider.call(
            {**request, "provider": self._config.provider, "model": self._config.model}
        )
        if not isinstance(response, dict) or not isinstance(response.get("content"), str):
            raise ValueError("provider response.content must be a string")
        return response

    def execute_tool(self, call: Dict[str, Any]) -> Dict[str, Any]:
        """Apply the captured allowlist before dispatching a registered Python tool handler."""
        name = call.get("tool-name")
        entry = self._entries.get(name) if isinstance(name, str) else None
        if entry is None or not entry["allow"]:
            raise PermissionError(f"permission: tool '{name}' is not allowed")
        args = json.loads(call.get("arguments-json", "{}"))
        error = None
        try:
            output = json.dumps(entry["handler"](args), allow_nan=False)
        except Exception as exc:
            output, error = "{}", str(exc)
        return {
            "tool-name": name,
            "success": error is None,
            "output-json": output,
            "error-message": error,
            "step-id": call.get("step-id", 0),
        }

    def export_client(
        self,
        destination: Path,
        *,
        server_url: Optional[str] = None,
        bindings_dir: Optional[Path] = None,
        client_tools: Optional[Dict[str, str]] = None,
        hooks_module: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Write a standalone client environment into a new directory.

        The bundle contains agent.json, agent.js, the raw composed agent.wasm,
        the jco factory, WASI adapters, and explicitly selected JS bindings.
        Provider instances, Python callbacks, credentials, and session history
        are not serialized. manifest.json inventories each artifact by SHA-256.
        """
        definition = copy.deepcopy(self.definition)
        definition["serverUrl"] = _server_url(server_url)
        mapping = client_tools or {}
        for name, module in mapping.items():
            if name not in self._entries:
                raise ValueError(f"unknown client tool binding '{name}'")
            _module_path(module)
        definition["agent"]["hooksModule"] = _module_path(hooks_module) if hooks_module else None
        if self._hooks is not None and not hooks_module:
            raise ValueError("configured server hooks require a hooks_module client binding")
        for tool in definition["agent"]["tools"]:
            if tool["definition"]["name"] in mapping:
                tool["execution"] = "client"
                tool["module"] = mapping[tool["definition"]["name"]]
        modules = list(mapping.values()) + ([hooks_module] if hooks_module else [])
        if modules and bindings_dir is None:
            raise ValueError("bindings_dir is required for client modules")
        for module in modules:
            assert bindings_dir is not None
            if not (Path(bindings_dir) / module).is_file():
                raise ValueError(f"client binding missing: {module}")
        component = (_PACKAGE / "antikythera.wasm").read_bytes()
        if component[:8] != b"\x00asm\x0d\x00\x01\x00":
            raise ValueError("delivery artifact must be a WASM Component Model binary")
        target = Path(destination).resolve()
        if target.exists():
            raise ValueError("export destination already exists; use a new directory")
        target.parent.mkdir(parents=True, exist_ok=True)
        stage = Path(tempfile.mkdtemp(prefix=".agent-export-", dir=target.parent))
        try:
            _copy_directory(_PACKAGE / "component", stage / "component")
            if bindings_dir is not None:
                _copy_directory(Path(bindings_dir).resolve(), stage / "bindings")
            (stage / "agent.wasm").write_bytes(component)
            shutil.copyfile(_PACKAGE / "delivery_assets/runtime.mjs", stage / "runtime.mjs")
            shutil.copyfile(_PACKAGE / "delivery_assets/client.d.ts", stage / "agent.d.ts")
            (stage / "package.json").write_text(
                '{"type":"module","private":true}', encoding="utf-8"
            )
            (stage / "agent.json").write_text(json.dumps(definition, indent=2), encoding="utf-8")
            entry = (
                "/** Creates this configured agent with a private jco component instance. */\n"
                "import { createConfiguredAgent } from './runtime.mjs';\n"
                f"const definition = {json.dumps(definition)};\n"
                "export { definition };\n"
                "export function createAgent(environment = {}) {\n"
                "  return createConfiguredAgent(definition, { ...environment, "
                "baseUrl: import.meta.url, "
                "componentUrl: new URL('./component/instantiate.js', import.meta.url).href });\n}\n"
            )
            (stage / "agent.js").write_text(entry, encoding="utf-8")
            manifest = {
                "schemaVersion": 1,
                "sdkVersion": get_version(),
                "agentId": definition["agent"]["id"],
                "entry": "agent.js",
                "configuration": "agent.json",
                "component": "agent.wasm",
                "jco": "component/instantiate.js",
                "files": [
                    {
                        "path": file.relative_to(stage).as_posix(),
                        "size": file.stat().st_size,
                        "sha256": hashlib.sha256(file.read_bytes()).hexdigest(),
                    }
                    for file in sorted(stage.rglob("*"))
                    if file.is_file()
                ],
            }
            (stage / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
            _install_export(stage, target)
            return manifest
        finally:
            if stage.exists():
                shutil.rmtree(stage)

    def serve(self, **options: Any) -> DeliveryServer:
        """Start an HTTP delivery host and return its lifecycle handle and client entry URL."""
        server = DeliveryServer(self, **options)
        server.start()
        return server


class DeliveryServer:
    """HTTP lifecycle for native execution, host capabilities, and public WASM/jco assets."""

    def __init__(
        self,
        deployment: AgentDeployment,
        *,
        bind: str = "127.0.0.1",
        port: int = 0,
        auth_token: Optional[str] = None,
        allowed_origins: Optional[list[str]] = None,
        public_url: Optional[str] = None,
        **export_options: Any,
    ) -> None:
        if bind not in ("127.0.0.1", "localhost", "::1") and not auth_token:
            raise ValueError("non-loopback delivery hosts require auth_token")
        self.deployment = deployment
        self.bind, self.port = bind, port
        self.auth_token = auth_token
        self.allowed_origins = allowed_origins or []
        self.public_url = _server_url(public_url)
        self.export_options = export_options
        self.url = ""
        self.client_url = ""
        self.manifest_url = ""
        self._http: Any = None
        self._thread: Any = None
        self._temporary: Any = None

    def start(self) -> str:
        """Bind the host and build the client export using its actual listening URL."""
        if self._http is not None:
            raise RuntimeError("delivery server is already started")
        owner = self

        class Handler(BaseHTTPRequestHandler):
            """Route bounded JSON requests and serve only files in the generated bundle."""

            def log_message(self, format: str, *args: Any) -> None:
                """Leave logging to the embedding application."""

            def _send(self, status: int, body: Any, mime: str = "application/json") -> None:
                data = body if isinstance(body, bytes) else json.dumps(body).encode()
                self.send_response(status)
                self.send_header("Content-Type", mime)
                self.send_header("Content-Length", str(len(data)))
                origin = self.headers.get("Origin")
                public = urlsplit(owner.public_url or owner.url)
                public_origin = f"{public.scheme}://{public.netloc}"
                if origin and (
                    origin in (owner.url, public_origin) or origin in owner.allowed_origins
                ):
                    self.send_header("Access-Control-Allow-Origin", origin)
                    self.send_header("Vary", "Origin")
                self.send_header("Cache-Control", "no-store")
                self.end_headers()
                self.wfile.write(data)

            def _origin_allowed(self) -> bool:
                origin = self.headers.get("Origin")
                public = urlsplit(owner.public_url or owner.url)
                public_origin = f"{public.scheme}://{public.netloc}"
                if (
                    origin
                    and origin not in (owner.url, public_origin)
                    and origin not in owner.allowed_origins
                ):
                    self._send(403, {"error": "origin is not allowed"})
                    return False
                return True

            def do_OPTIONS(self) -> None:
                """Answer browser preflight only for the configured origins."""
                if not self._origin_allowed():
                    return
                self.send_response(204)
                self.send_header(
                    "Access-Control-Allow-Origin", self.headers.get("Origin", owner.url)
                )
                self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
                self.send_header("Access-Control-Allow-Headers", "Content-Type, Authorization")
                self.end_headers()

            def do_GET(self) -> None:
                """Expose the intentionally public configuration and static component assets."""
                if not self._origin_allowed():
                    return
                route = unquote(urlsplit(self.path).path)
                root = Path(owner._temporary.name) / "bundle"
                target = (root / route.removeprefix("/bundle/")).resolve()
                if (
                    not route.startswith("/bundle/")
                    or root not in target.parents
                    or not target.is_file()
                ):
                    self._send(404, {"error": "bundle file not found"})
                    return
                mime = (
                    "application/wasm"
                    if target.suffix == ".wasm"
                    else (
                        "text/javascript"
                        if target.suffix in (".mjs", ".js")
                        else mimetypes.guess_type(str(target))[0] or "application/octet-stream"
                    )
                )
                self._send(200, target.read_bytes(), mime)

            def do_POST(self) -> None:
                """Serve host capabilities while retaining credentials on the server."""
                try:
                    length = int(self.headers.get("Content-Length", "0"))
                    if length < 0 or length > 1024 * 1024:
                        self._send(413, {"error": "request exceeds 1 MiB"})
                        return
                    self.connection.settimeout(
                        min(5.0, max(0.001, owner.deployment._config.timeout / 1000))
                    )
                    raw_body = self.rfile.read(length)
                    if len(raw_body) != length:
                        raise ValueError("request body is incomplete")
                except (TypeError, ValueError) as exc:
                    self._send(400, {"error": str(exc)})
                    return
                except TimeoutError:
                    self._send(408, {"error": "request body timed out"})
                    return
                if not self._origin_allowed():
                    return
                route = urlsplit(self.path).path
                if route not in ("/run", "/llm/call", "/tools/execute"):
                    self._send(404, {"error": "route not found"})
                    return
                if (
                    owner.auth_token
                    and self.headers.get("Authorization") != f"Bearer {owner.auth_token}"
                ):
                    self._send(401, {"error": "authentication required"})
                    return
                try:
                    body = json.loads(raw_body)
                    if not isinstance(body, dict):
                        raise ValueError("request must be an object")
                    if route == "/run":
                        prompt = body.get("prompt")
                        if not isinstance(prompt, str) or not prompt.strip():
                            raise ValueError("prompt must be a non-empty string")
                        result = asdict(owner.deployment.create_agent().run(prompt))
                        result = {
                            "output": result["output"],
                            "success": result["success"],
                            "stepsUsed": result["steps_used"],
                            "sessionId": result["session_id"],
                            "error": result["error"],
                        }
                    elif route == "/llm/call":
                        result = owner.deployment.call_provider(body)
                    else:
                        result = owner.deployment.execute_tool(body)
                    self._send(200, result)
                except PermissionError as exc:
                    self._send(403, {"error": str(exc)})
                except (TypeError, ValueError) as exc:
                    self._send(400, {"error": str(exc)})
                except Exception as exc:
                    self._send(500, {"error": str(exc)})

        try:

            class HttpServer(ThreadingHTTPServer):
                """Match the listener address family to the configured IPv4 or IPv6 bind."""

                address_family = socket.AF_INET6 if ":" in self.bind else socket.AF_INET

            self._http = HttpServer((self.bind, self.port), Handler)
            self._http.daemon_threads = True
            hostname = f"[{self.bind}]" if ":" in self.bind else self.bind
            self.url = f"http://{hostname}:{self._http.server_port}"
            self._temporary = tempfile.TemporaryDirectory(prefix="antikythera-delivery-")
            self.deployment.export_client(
                Path(self._temporary.name) / "bundle",
                server_url=self.public_url or self.url,
                **self.export_options,
            )
            self.client_url = f"{self.public_url or self.url}/bundle/agent.js"
            self.manifest_url = f"{self.public_url or self.url}/bundle/manifest.json"
            self._thread = threading.Thread(target=self._http.serve_forever, daemon=True)
            self._thread.start()
            return self.url
        except Exception:
            if self._http is not None:
                self._http.server_close()
                self._http = None
            if self._temporary is not None:
                self._temporary.cleanup()
            raise

    def close(self) -> None:
        """Stop serving and delete only the temporary export owned by this handle."""
        if self._http is not None:
            self._http.shutdown()
            self._http.server_close()
            self._thread.join(timeout=5)
            self._http = None
        if self._temporary is not None:
            self._temporary.cleanup()
            self._temporary = None

    def __enter__(self) -> DeliveryServer:
        """Start an unstarted handle when used as a context manager."""
        if self._http is None:
            self.start()
        return self

    def __exit__(self, *args: Any) -> None:
        """Release the HTTP host and its temporary bundle on context exit."""
        self.close()
