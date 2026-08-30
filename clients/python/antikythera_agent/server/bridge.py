"""Facade server runtime bridge (unit U32): createAgentServer + AgentServer + CLI.

Sumber kebenaran:
- documentation/DECISIONS_RUNTIME_BRIDGE.md  (D2 drop-in peer; D3 transport
  default di belakang port interface; D4 manifest + bundle jco; D6 wasmtime
  OPTIONAL — core@server butuh wasm_path; core@client jalan tanpa wasmtime)
- documentation/WIRE_PROTOCOL.md              (§2 endpoint; §7 nol field di
  luar golden; deny = `permission:` invariant R4)
- antikythera-server-runtime/src/main.rs      (flag CLI mirror; `--server-tool`
  = registrasi = grant; `--allow-tool` = allowlist server+client+mcp; baris
  `[server-runtime] HTTP wire bridge listening on <url>`)
- antikythera-server-runtime/src/config.rs    (ServerToolSpec::parse: nama
  sebelum titik dua PERTAMA; GatePolicy deny-all)
- antikythera-server-runtime/src/core.rs      (definisi tool `--server-tool`:
  description "Server tool registered via --server-tool", input_schema
  object kosong)
- Unit hilir yang SUDAH ADA (kontrak sambungan dibaca dari docstring):
  wire.py, registry.py, gate.py, provider.py, control.py, component.py,
  transport.py, loop_owner.py, antikythera_agent/runtime.py, host.py.

Komposisi:
- `createAgentServer(options)` adalah SATU-SATUNYA titik komposisi: ia
  memvalidasi `AgentServerOptions` di entry point, membangun dependensi
  (registry/gate/control/component/transport/provider resolver), dan
  mengembalikan `AgentServer` ter-wire.
- `AgentServer` memaparkan lifecycle (`start`/`stop`/`url`), registrasi tool
  (`register_server_tool`/`register_client_tools`), hook MCP (ditolak —
  transport MCP belum ada), dan loop core@server (`run_server_loop`).

Amplop runtime:
- Base install zero-dependency (D6): wasmtime TIDAK di-import oleh modul ini;
  `WasmRuntime` di-import lokal di `run_server_loop` dan wasmtime hanya
  disentuh oleh `_ensure_initialized` pada panggilan runner pertama.
- `wasm_path` diberikan → mode core@server TERSEDIA; tanpa `wasm_path`
  `run_server_loop` gagal eksplisit (bukan degradasi senyap) — server tetap
  berjalan sebagai peer core@client (static + wire).
- `providers` di-merge di atas registry default provider.py (stub+ollama);
  registry modul-global TIDAK pernah dimutasi (salinan per server).
- Default gate = deny-all (R4); `policy` hanya menggantikan gate saat
  diberikan.
"""

from __future__ import annotations

import ipaddress
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional, Union

from .component import ComponentServer
from .control import ControlChannel
from .gate import PolicyGate
from .loop_owner import LoopOutcome, ToolLoopConfig, run_tool_loop
from .provider import LlmProvider, OllamaProvider, StubProvider, provider_registry
from .registry import UnionRegistry
from .transport import DEFAULT_CLIENT_ID, ThreadingHttpTransport

#: Deskripsi definisi tool yang didaftarkan CLI `--server-tool` (mirror
#: antikythera-server-runtime/src/core.rs).
SERVER_TOOL_DESCRIPTION = "Server tool registered via --server-tool"


@dataclass
class AgentServerOptions:
    """Parameter konfigurasi server; DIVALIDASI di entry point createAgentServer.

    - `bind` — host bind (IPv4, hostname, atau literal IPv6). Port TIDAK
      boleh di sini (kolon = IPv6 atau kesalahan pemanggil); pakai `port`.
    - `port` — 0 = ephemeral; `start()` mengembalikan port aktual.
    - `component_dir` — direktori bundle jco; None = paket
      `antikythera_agent/component` (D1, wheel self-contained).
    - `wasm_path` — komposit WASM; HANYA untuk mode core@server (D6).
      Harus file yang ADA; None = mode core@server tidak tersedia.
    - `providers` — nama → instance `LlmProvider` ATAU spec dict:
      `{"type": "stub", "response": "<json-string>"}` atau
      `{"type": "ollama", "base_url": str, "model": str|None}`.
      Di-merge di atas registry default provider.py (stub + ollama).
    - `default_provider` — nama provider default (harus ada setelah merge).
    - `policy` — `PolicyGate`; None = gate kosong default-deny (R4).
    - `client_id` — identitas peer SSE yang server harapkan; None = default
      transport ("client-a", golden `llm_token_event`).
    - `max_steps` — batas iterasi LLM loop (non-negatif).
    - `runtime_hooks_enabled` — diteruskan ke `run_server_loop` (core@server).
    - `corr_ttl_secs` — TTL POST-back (WIRE_PROTOCOL §5); diteruskan ke loop
      sebagai `pending_ttl_secs`.
    - `keepalive_secs` — interval keepalive SSE control channel.
    """

    bind: str = "127.0.0.1"
    port: int = 0
    component_dir: Optional[Path] = None
    wasm_path: Optional[Path] = None
    providers: Optional[Dict[str, Union[LlmProvider, Dict[str, Any]]]] = None
    default_provider: str = "stub"
    policy: Optional[PolicyGate] = None
    client_id: Optional[str] = None
    max_steps: int = 10
    runtime_hooks_enabled: bool = True
    corr_ttl_secs: int = 60
    keepalive_secs: int = 15


def _validate_options(options: AgentServerOptions) -> None:
    """Entry-point guard: invarian struktur sebelum komposisi dependensi."""
    if not isinstance(options.bind, str) or not options.bind.strip():
        raise ValueError("AgentServerOptions.bind must be a non-empty host string")
    if any(ch.isspace() for ch in options.bind):
        raise ValueError(
            f"AgentServerOptions.bind must not contain whitespace: {options.bind!r}"
        )
    if ":" in options.bind:
        # Kolon sah hanya sebagai literal IPv6; port bukan urusan bind
        # (misuse `"127.0.0.1:8787"` = port di bind → program error eksplisit).
        try:
            ipaddress.IPv6Address(options.bind)
        except ValueError:
            raise ValueError(
                f"AgentServerOptions.bind contains ':' but is not a valid IPv6 "
                f"literal: {options.bind!r} (pass the port via AgentServerOptions.port)"
            ) from None
    if (
        not isinstance(options.port, int)
        or isinstance(options.port, bool)
        or not (0 <= options.port <= 65535)
    ):
        raise ValueError(
            f"AgentServerOptions.port must be an integer in [0, 65535], got {options.port!r}"
        )
    if options.component_dir is not None:
        Path(options.component_dir)
    if options.wasm_path is not None:
        wasm = Path(options.wasm_path)
        if not wasm.is_file():
            raise ValueError(
                f"AgentServerOptions.wasm_path does not exist or is not a file: {wasm}"
            )
    if options.providers is not None and not isinstance(options.providers, dict):
        raise TypeError("AgentServerOptions.providers must be a dict or None")
    if not isinstance(options.default_provider, str) or not options.default_provider:
        raise ValueError("AgentServerOptions.default_provider must be a non-empty string")
    if options.policy is not None and not isinstance(options.policy, PolicyGate):
        raise TypeError("AgentServerOptions.policy must be a PolicyGate or None")
    if options.client_id is not None and (
        not isinstance(options.client_id, str) or not options.client_id
    ):
        raise ValueError("AgentServerOptions.client_id must be a non-empty string or None")
    if (
        not isinstance(options.max_steps, int)
        or isinstance(options.max_steps, bool)
        or options.max_steps < 0
    ):
        raise ValueError("AgentServerOptions.max_steps must be a non-negative integer")
    if not isinstance(options.runtime_hooks_enabled, bool):
        raise TypeError("AgentServerOptions.runtime_hooks_enabled must be a bool")
    if (
        not isinstance(options.corr_ttl_secs, (int, float))
        or isinstance(options.corr_ttl_secs, bool)
        or options.corr_ttl_secs <= 0
    ):
        raise ValueError("AgentServerOptions.corr_ttl_secs must be a positive number")
    if (
        not isinstance(options.keepalive_secs, (int, float))
        or isinstance(options.keepalive_secs, bool)
        or options.keepalive_secs <= 0
    ):
        raise ValueError("AgentServerOptions.keepalive_secs must be a positive number")


def _coerce_provider(name: str, spec: Any) -> LlmProvider:
    """Konversi spec dict ke instance LlmProvider; instance diterima apa adanya.

    Spec dict (`{"type": "stub" | "ollama", ...}`) adalah kontrak facade baru
    (bukan wire-facing); bentuknya didokumentasikan di `AgentServerOptions`.
    """
    if isinstance(spec, LlmProvider):
        return spec
    if isinstance(spec, dict):
        kind = spec.get("type")
        if kind == "stub":
            response = spec.get("response")
            if not isinstance(response, str):
                raise ValueError(
                    f"AgentServerOptions.providers[{name!r}]: stub spec requires a "
                    "'response' JSON string"
                )
            return StubProvider(response)
        if kind == "ollama":
            base_url = spec.get("base_url", "http://127.0.0.1:11434")
            model = spec.get("model")
            if not isinstance(base_url, str) or not base_url:
                raise ValueError(
                    f"AgentServerOptions.providers[{name!r}]: ollama spec requires a "
                    "non-empty 'base_url'"
                )
            if model is not None and not isinstance(model, str):
                raise ValueError(
                    f"AgentServerOptions.providers[{name!r}]: ollama 'model' must be a "
                    "string or None"
                )
            return OllamaProvider(base_url=base_url, model=model)
        raise ValueError(
            f"AgentServerOptions.providers[{name!r}]: unknown provider spec type "
            f"{kind!r}; expected 'stub' or 'ollama'"
        )
    raise TypeError(
        f"AgentServerOptions.providers[{name!r}] must be an LlmProvider or a spec "
        f"dict, got {type(spec).__name__}"
    )


def _build_provider_resolver(
    providers: Dict[str, LlmProvider], default_provider: str
) -> Callable[[Optional[str]], LlmProvider]:
    """Resolver `(name) -> LlmProvider` di atas peta provider server.

    Pesan KeyError menyamai `provider.resolve_provider` — konsumen hilir
    (transport handle_llm_call 400, loop_owner) membaca `KeyError.args[0]`.
    """

    def resolve(name: Optional[str] = None) -> LlmProvider:
        key = default_provider if name is None else name
        try:
            return providers[key]
        except KeyError:
            raise KeyError(
                f"unknown LLM provider: {key!r}; known: {sorted(providers)}"
            ) from None

    return resolve


def createAgentServer(
    options: Optional[Union[AgentServerOptions, Dict[str, Any]]] = None,
) -> "AgentServer":
    """Komposisi dependensi server + validasi entry point (U32).

    `options` menerima `AgentServerOptions` ATAU dict nilai options (kontrak
    facade: "dataclass atau dict yang divalidasi di entry point"). Dict
    dikonversi via `AgentServerOptions(**options)` — kunci tak dikenal
    ditolak TypeError, nilai invalid ditolak `_validate_options`. Memvalidasi
    `AgentServerOptions`, membangun registry/gate/control/component/
    transport/provider resolver, dan mengembalikan `AgentServer` siap-`start`.
    Peta provider di-salin dari registry modul-global provider.py — registry
    modul tidak pernah dimutasi.
    """
    if options is None:
        options = AgentServerOptions()
    if isinstance(options, dict):
        options = AgentServerOptions(**options)
    if not isinstance(options, AgentServerOptions):
        raise TypeError(
            "createAgentServer options must be an AgentServerOptions or a dict, "
            f"got {type(options).__name__}"
        )
    _validate_options(options)

    providers: Dict[str, LlmProvider] = dict(provider_registry)
    for name, spec in (options.providers or {}).items():
        providers[name] = _coerce_provider(name, spec)
    if options.default_provider not in providers:
        raise ValueError(
            f"default_provider {options.default_provider!r} is not a registered "
            f"provider; known: {sorted(providers)}"
        )

    registry = UnionRegistry()
    gate = options.policy if options.policy is not None else PolicyGate()
    control = ControlChannel(keepalive_interval=float(options.keepalive_secs))
    component = ComponentServer(bundle_dir=options.component_dir)
    client_id = options.client_id if options.client_id is not None else DEFAULT_CLIENT_ID
    provider_resolver = _build_provider_resolver(providers, options.default_provider)
    transport = ThreadingHttpTransport(
        registry=registry,
        gate=gate,
        control=control,
        component=component,
        provider_resolver=provider_resolver,
        client_id=client_id,
    )
    return AgentServer(
        options=options,
        registry=registry,
        gate=gate,
        control=control,
        component=component,
        transport=transport,
        provider_resolver=provider_resolver,
        client_id=client_id,
    )


class AgentServer:
    """Lifecycle facade server runtime bridge (unit U32).

    Komposisi dependensi dilakukan `createAgentServer`; instance memegang
    referensi unit dan memaparkan lifecycle HTTP + registrasi tool + loop
    core@server. `start()`/`stop()` idempotent-aman; `url()` sebelum start
    gagal eksplisit (bukan degradasi senyap).
    """

    def __init__(
        self,
        *,
        options: AgentServerOptions,
        registry: UnionRegistry,
        gate: PolicyGate,
        control: ControlChannel,
        component: ComponentServer,
        transport: ThreadingHttpTransport,
        provider_resolver: Callable[[Optional[str]], LlmProvider],
        client_id: str,
    ) -> None:
        self._options = options
        self._registry = registry
        self._gate = gate
        self._control = control
        self._component = component
        self._transport = transport
        self._provider_resolver = provider_resolver
        self._client_id = client_id
        self._server_url: Optional[str] = None
        self._wasm_path: Optional[Path] = (
            Path(options.wasm_path).resolve() if options.wasm_path is not None else None
        )

    # -- properti ---------------------------------------------------------

    @property
    def registry(self) -> UnionRegistry:
        return self._registry

    @property
    def gate(self) -> PolicyGate:
        return self._gate

    @property
    def control(self) -> ControlChannel:
        return self._control

    @property
    def transport(self) -> ThreadingHttpTransport:
        return self._transport

    @property
    def component(self) -> ComponentServer:
        return self._component

    @property
    def server_url(self) -> Optional[str]:
        """URL HTTP aktif, atau None sebelum `start()` (aksesor state mentah)."""
        return self._server_url

    # -- lifecycle --------------------------------------------------------

    @staticmethod
    def _format_url(bind: str, port: int) -> str:
        """URL wire `http://<bind>:<port>`; IPv6 dibungkus kurung siku."""
        host = f"[{bind}]" if ":" in bind else bind
        return f"http://{host}:{port}"

    def start(self) -> str:
        """Bind + layani HTTP di thread daemon; mengembalikan base URL.

        `port=0` (ephemeral) → port aktual dari socket server dilaporkan
        (kontrak: start() -> base URL dengan port aktual). Start ganda gagal
        eksplisit.
        """
        if self._server_url is not None:
            raise RuntimeError("AgentServer is already started")
        port = self._transport.start(self._options.bind, self._options.port)
        self._server_url = self._format_url(self._options.bind, port)
        return self._server_url

    def stop(self) -> None:
        """Tutup HTTP server + control channel; idempotent.

        `control.close()` adalah seam shutdown publik (control.py): men-set
        event stop keepalive DAN join thread daemon. Setelah stop,
        `start()` dapat dipanggil lagi (HTTP restart; keepalive control
        tidak restartable — SSE hidup tanpa keepalive setelah restart).
        """
        self._transport.stop()
        self._control.close()
        self._server_url = None

    def url(self) -> str:
        """Base URL aktif; RuntimeError bila server belum di-start."""
        if self._server_url is None:
            raise RuntimeError("AgentServer is not started; call start() first")
        return self._server_url

    # -- registrasi tool --------------------------------------------------

    def register_server_tool(
        self,
        definition: Dict[str, Any],
        handler: Optional[Callable[..., Any]] = None,
    ) -> None:
        """Daftarkan tool dieksekusi server-side (owner `server`, R5).

        Registrasi TIDAK meng-allowlist: gate default-deny tetap menolak
        sampai `gate.allow_tool("server", name)` dipanggil (CLI `--server-tool`
        adalah grant eksplisit — mirror main.rs).
        """
        self._registry.register_server(definition, handler)

    def register_client_tools(self, definitions: List[Dict[str, Any]]) -> None:
        """Daftarkan definisi tool yang dieksekusi peer client (owner `client`)."""
        if not isinstance(definitions, list) or not all(
            isinstance(d, dict) for d in definitions
        ):
            raise TypeError(
                "register_client_tools definitions must be a list of dicts"
            )
        for definition in definitions:
            self._registry.register_client(definition)

    def register_tool(self, obj: Any, allow: bool = False) -> None:
        """Daftarkan Tool sugar (BaseTool subclass atau @tool function).

        Delegasi ke `antikythera_agent.tools.helpers.register_tool` dengan
        `registry=self._registry` dan `gate=self._gate` (register != allow
        strict; collision existing-first dijaga registry; 6-key golden di-
        validasi helpers). `allow=True` sekalian allowlist di gate,
        `allow=False` hanya register tanpa allow.

        Args:
            obj: subclass `BaseTool` atau function `@tool`.
            allow: bila True sekalian `gate.allow_tool(owner, name)`.

        Raises:
            TypeError: bila obj bukan Tool atau allow bukan bool.
            ValueError: owner/keys invalid atau collision registry.
        """
        from antikythera_agent.tools.helpers import register_tool as _register_tool

        _register_tool(self._registry, self._gate, obj, allow=allow)

    def register_tools(self, objs: Any, allow: bool = False) -> None:
        """Bulk helper untuk `register_tool` (loop; allow berlaku untuk semua).

        Args:
            objs: iterable Tool sugar.
            allow: bila True sekalian allowlist tiap tool di gate.
        """
        if not isinstance(allow, bool):
            raise TypeError("register_tools: 'allow' must be a bool")
        if objs is None or isinstance(objs, (str, bytes)):
            raise TypeError("register_tools: objs must be an iterable of Tools")
        try:
            items = list(objs)
        except TypeError:
            raise TypeError("register_tools: objs must be an iterable of Tools") from None
        for obj in items:
            self.register_tool(obj, allow=allow)

    def connect_mcp_server(self, config: Any) -> None:
        """Sambungkan MCP server (destination `mcp`, selalu server-side, K2).

        Transport MCP BELUM ada di paket Python — unit ini sengaja TIDAK
        mengimplementasikannya; kontrak eksplisit untuk unit transport MCP
        mendatang. Mencoba memanggil = NotImplementedError.
        """
        raise NotImplementedError(
            "MCP transport is not implemented in the Python server runtime; "
            "connect_mcp_server is reserved for a future MCP transport unit"
        )

    # -- loop core@server -------------------------------------------------

    def run_server_loop(self, config: ToolLoopConfig) -> LoopOutcome:
        """Jalankan tool loop di komposit (mode core@server, K1).

        Membutuhkan `AgentServerOptions.wasm_path` (D6): tanpanya mode
        core@server tidak tersedia → RuntimeError eksplisit. Menggunakan
        `run_tool_loop` dari loop_owner.py dengan `WasmRuntime` fresh per
        run; `runtime_hooks_enabled`/`client_id`/`pending_ttl_secs` server
        diteruskan ke config loop (identitas peer + TTL adalah milik server).
        """
        if not isinstance(config, ToolLoopConfig):
            raise TypeError(
                f"run_server_loop config must be a ToolLoopConfig, "
                f"got {type(config).__name__}"
            )
        if self._wasm_path is None:
            raise RuntimeError(
                "core@server mode is not available: AgentServerOptions.wasm_path "
                "is not set; pass the composite WASM path and install wasmtime "
                "(pip install antikythera-agent[wasm])"
            )
        # Import lokal: wasmtime hanya disentuh saat mode core@server
        # diaktifkan (D6); base install tetap zero-dependency.
        from antikythera_agent.runtime import WasmRuntime

        runtime = WasmRuntime()
        # Seam runtime.py: _ensure_initialized membaca self._wasm_path secara
        # lazy — override instance sebelum panggilan runner pertama.
        runtime._wasm_path = self._wasm_path
        effective = replace(
            config,
            runtime_hooks_enabled=self._options.runtime_hooks_enabled,
            client_id=self._client_id,
            pending_ttl_secs=float(self._options.corr_ttl_secs),
        )
        return run_tool_loop(
            runtime,
            self._registry,
            self._gate,
            self._provider_resolver,
            self._control,
            effective,
        )
