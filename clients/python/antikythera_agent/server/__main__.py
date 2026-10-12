"""CLI server runtime bridge: `python -m antikythera_agent.server`.

Mirror flag antikythera-server-runtime/src/main.rs (kontrak U61 parity test).
Setelah start mencetak baris PERSIS:

    [server-runtime] HTTP wire bridge listening on <url>

Parity test JS mencari substring "listening on"; `url` memakai format
`http://<bind>` dengan port aktual (`start()` melaporkan port socket nyata).

Flag:
- `--bind <addr:port>`        default "127.0.0.1:8787" (config.rs default)
- `--provider-stub <json>`    ganti provider "stub" (StubProvider) + default
- `--server-tool <name>:<json>`  daftarkan tool server; registrasi = grant
- `--allow-tool <name>`       allowlist destination server+client+mcp
- `--client-id <id>`          default "antikythera-client" (main.rs default)
- `--component-dir <path>`    direktori bundle jco (D4/D1)
- `--wasm-path <path>`        komposit WASM → mode core@server tersedia (D6)
- `--max-steps <n>`           batas iterasi LLM loop
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import threading
from pathlib import Path
from typing import Any, Dict, Optional, Sequence, Tuple

from .bridge import SERVER_TOOL_DESCRIPTION, AgentServerOptions, createAgentServer
from .provider import StubProvider

#: Default bind CLI — mirror ServerRuntimeConfig::default() (config.rs).
DEFAULT_BIND = "127.0.0.1:8787"

#: Default client id CLI — mirror main.rs (peer SSE yang server harapkan).
DEFAULT_CLIENT_ID = "antikythera-client"

#: Default max-steps CLI — mirror ToolLoopConfig::default() (loop_owner.rs).
DEFAULT_MAX_STEPS = 10


def parse_bind(value: str) -> Tuple[str, int]:
    """Parse `<addr>:<port>`; IPv6 literal boleh dibungkus kurung siku."""
    host, sep, port_str = value.rpartition(":")
    if not sep:
        raise ValueError(f"invalid --bind '{value}': expected <addr>:<port>")
    if host.startswith("[") and host.endswith("]"):
        host = host[1:-1]
    if not host:
        raise ValueError(f"invalid --bind '{value}': address must not be empty")
    try:
        port = int(port_str)
    except ValueError:
        raise ValueError(f"invalid --bind '{value}': port must be an integer")
    if not (0 <= port <= 65535):
        raise ValueError(f"invalid --bind '{value}': port must be in [0, 65535]")
    return host, port


def parse_server_tool(spec: str) -> Tuple[str, Any]:
    """Parse `<name>:<response-json>` — nama = teks sebelum titik dua PERTAMA
    (mirror `ServerToolSpec::parse`, config.rs); sisa harus JSON valid."""
    colon = spec.find(":")
    if colon == -1:
        raise ValueError(f"invalid --server-tool '{spec}': expected <name>:<response-json>")
    name = spec[:colon].strip()
    if not name:
        raise ValueError(f"invalid --server-tool '{spec}': tool name must not be empty")
    try:
        response = json.loads(spec[colon + 1 :])
    except json.JSONDecodeError as exc:
        raise ValueError(f"invalid --server-tool response-json for tool '{name}': {exc}") from exc
    return name, response


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="python -m antikythera_agent.server",
        description=(
            "Antikythera HTTP wire bridge server (drop-in peer of the Rust server runtime, D2)."
        ),
    )
    parser.add_argument(
        "--bind",
        default=DEFAULT_BIND,
        metavar="ADDR:PORT",
        help="HTTP bind address (default: %(default)s)",
    )
    parser.add_argument(
        "--provider-stub",
        metavar="JSON",
        help="stub provider response JSON; registers provider 'stub' as default",
    )
    parser.add_argument(
        "--server-tool",
        action="append",
        metavar="NAME:JSON",
        help="register a server tool returning the static JSON; registration "
        "is a grant (auto-allowlist local)",
    )
    parser.add_argument(
        "--allow-tool",
        action="append",
        metavar="NAME",
        help="allowlist tool for server+client+mcp destinations",
    )
    parser.add_argument(
        "--client-id",
        metavar="ID",
        help=f"peer client id expected on SSE (default: {DEFAULT_CLIENT_ID})",
    )
    parser.add_argument("--component-dir", metavar="PATH", help="jco bundle directory")
    parser.add_argument(
        "--wasm-path",
        metavar="PATH",
        help="composite WASM path; enables core@server mode (D6)",
    )
    parser.add_argument(
        "--max-steps",
        type=int,
        metavar="N",
        help="LLM loop max steps (default: %(default)s)",
        default=DEFAULT_MAX_STEPS,
    )
    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    """Mirror main.rs: --provider-stub mengganti provider "stub" (StubProvider) dan menjadikannya
    default.

    Registrasi = grant (mirror main.rs): allowlist server otomatis sehingga gate default-deny
    tidak menolak tool terdaftar.

    Mirror main.rs: allowlist ketiga destination (local/remote/mcp).

    Blokir selamanya (mirror `std::thread::park()` main.rs); Ctrl+C menutup server dan keluar
    bersih.
    """
    args = build_parser().parse_args(argv)
    server = None
    try:
        bind, port = parse_bind(args.bind)
        providers: Optional[Dict[str, Any]] = None
        if args.provider_stub is not None:
            providers = {"stub": StubProvider(args.provider_stub)}
        options = AgentServerOptions(
            auth_token=os.environ.get("ANTIKYTHERA_AUTH_TOKEN"),
            allowed_origins=[
                value.strip()
                for value in os.environ.get("ANTIKYTHERA_ALLOWED_ORIGINS", "").split(",")
                if value.strip()
            ],
            bind=bind,
            port=port,
            component_dir=Path(args.component_dir) if args.component_dir else None,
            wasm_path=Path(args.wasm_path) if args.wasm_path else None,
            providers=providers,
            default_provider="stub",
            client_id=args.client_id if args.client_id is not None else DEFAULT_CLIENT_ID,
            max_steps=args.max_steps,
        )
        server = createAgentServer(options)
        for spec in args.server_tool or []:
            name, response = parse_server_tool(spec)
            server.register_server_tool(
                {"name": name, "description": SERVER_TOOL_DESCRIPTION},
                handler=lambda _args, response=response: response,
            )
            server.gate.allow_tool("server", name)
        for name in args.allow_tool or []:
            server.gate.allow_tool("server", name)
            server.gate.allow_tool("client", name)
            server.gate.allow_tool("mcp", name)
        url = server.start()
    except (ValueError, TypeError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    print(f"[server-runtime] HTTP wire bridge listening on {url}", flush=True)
    try:
        threading.Event().wait()
    except KeyboardInterrupt:
        return 0
    finally:
        if server is not None:
            server.stop()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
