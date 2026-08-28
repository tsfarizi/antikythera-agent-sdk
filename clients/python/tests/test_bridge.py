"""Falsification suite untuk facade `antikythera_agent.server.bridge` (unit U32).

Peran Coder — verifikasi mekanis WAJIB sebelum deklarasi selesai. Suite ini
menguji kontrak sambungan U32 yang dikonsumsi unit hilir (U61 parity test
runtime-bridge.test.mjs, U62 E2E manifest + bundle):

- Validasi `AgentServerOptions` di entry point: bind invalid, port invalid,
  wasm_path tak ada, provider spec tak dikenal, default_provider tak dikenal.
- Lifecycle: `start()` mengembalikan base URL aktif (port aktual untuk
  port=0); `url()` sebelum start gagal eksplisit; `stop()` menutup socket;
  idempotent; restart setelah stop.
- Registrasi tool server: `POST /tools/execute` dengan allowlist → sukses
  shape golden; deny default → 403 `permission:` persis `error_event.payload`.
- `GET /antikythera/v1/component/manifest` → shape golden `component_manifest`.
- CLI spawn: subprocess `python -m antikythera_agent.server` — baris
  "listening on" persis, `--provider-stub` menjawab `POST /llm/call` shape
  golden, `--server-tool` terdaftar di `GET /tools` + eksekusi nyata,
  `--allow-tool` tanpa registrasi tetap fail-closed 403.
- Loop core@server (`run_server_loop`): komposit nyata + stub → outcome
  `final`; tanpa `wasm_path` gagal eksplisit; `runtime_hooks_enabled`
  diteruskan (default true → denial `permission: hook` tanpa allowlist).

Sumber kebenaran:
- documentation/DECISIONS_RUNTIME_BRIDGE.md (D2/D3/D4/D6)
- documentation/WIRE_PROTOCOL.md (§2/§7)
- contracts/shared/wire_protocol.golden.json
- antikythera-server-runtime/src/main.rs + config.rs (flag CLI, ServerToolSpec)
- npm/antikythera-sdk/test/runtime-bridge.test.mjs (parity client-side)

Amplop test (asumsi yang dideklarasikan agar sertifikasi tetap sah):
- HTTP nyata memakai urllib.request stdlib + port ephemeral.
- CLI spawn memakai `sys.executable` (interpreter yang sama) dengan
  PYTHONPATH ke `python/`; proses dibunuh di teardown.
- Uji loop core@server di-skip LOUDLY bila komposit paket
  `python/antikythera_agent/antikythera.wasm` atau wasmtime tidak ada.

Menjalankan (dari repo root):
    $env:PYTHONPATH="python"
    python -m pytest python/tests/test_bridge.py -v
"""

from __future__ import annotations

import json
import os
import queue
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

import pytest

from antikythera_agent.server import (
    AgentServer,
    AgentServerOptions,
    ComponentServer,
    ControlChannel,
    PolicyGate,
    ThreadingHttpTransport,
    UnionRegistry,
    createAgentServer,
)
from antikythera_agent.server.loop_owner import ToolLoopConfig

_PYTHON_SRC = Path(__file__).resolve().parents[1]
_REPO_ROOT = Path(__file__).resolve().parents[3]

_GOLDEN_PATH = _REPO_ROOT / "contracts" / "shared" / "wire_protocol.golden.json"
GOLDEN = json.loads(_GOLDEN_PATH.read_text(encoding="utf-8"))

_PACKAGED_WASM = _PYTHON_SRC / "antikythera_agent" / "antikythera.wasm"

LLM_REQUEST_STUB_PROVIDER = dict(GOLDEN["llm_call_request"], provider=None)


# ===========================================================================
# Helper HTTP (pola test_transport.py) + CLI spawn
# ===========================================================================

def _request(method: str, url: str, body: Optional[Dict[str, Any]] = None) -> Tuple[int, bytes, Optional[str]]:
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


def _json(raw: bytes) -> Any:
    return json.loads(raw.decode("utf-8"))


def _free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _wait_for_line(proc: subprocess.Popen, needle: str, timeout: float = 20.0) -> str:
    """Baca stdout baris-demi-baris di thread; timeout melindungi hung spawn."""
    q: "queue.Queue[Optional[str]]" = queue.Queue()

    def reader() -> None:
        try:
            for line in proc.stdout:  # type: ignore[union-attr]
                q.put(line)
        finally:
            q.put(None)

    threading.Thread(target=reader, daemon=True).start()
    lines: List[str] = []
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            line = q.get(timeout=deadline - time.monotonic())
        except queue.Empty:
            break
        if line is None:
            break
        lines.append(line)
        if needle in line:
            return line
    stderr = ""
    if proc.stderr is not None:
        stderr = proc.stderr.read()
    raise AssertionError(
        f"CLI did not print {needle!r} within {timeout}s "
        f"(exit code {proc.poll()}): stdout={''.join(lines)} stderr={stderr}"
    )


class _CliProcess:
    """Proses CLI yang berjalan + URL wire; bunuh di teardown."""

    def __init__(self, proc: subprocess.Popen, url: str, port: int, listening_line: str) -> None:
        self.proc = proc
        self.url = url
        self.port = port
        self.listening_line = listening_line

    def stop(self) -> None:
        if self.proc.poll() is None:
            self.proc.kill()
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            pass


def start_cli(args: List[str], port: int) -> _CliProcess:
    """Spawn `python -m antikythera_agent.server` dan tunggu "listening on"."""
    env = dict(os.environ)
    env["PYTHONPATH"] = str(_PYTHON_SRC) + os.pathsep + env.get("PYTHONPATH", "")
    proc = subprocess.Popen(
        [sys.executable, "-m", "antikythera_agent.server", "--bind", f"127.0.0.1:{port}", *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        cwd=str(_REPO_ROOT),
        text=True,
    )
    try:
        line = _wait_for_line(proc, "listening on")
    except Exception:
        proc.kill()
        raise
    expected = f"[server-runtime] HTTP wire bridge listening on http://127.0.0.1:{port}"
    assert line.strip() == expected, f"unexpected listening line: {line!r}"
    return _CliProcess(proc, f"http://127.0.0.1:{port}", port, line)


# ===========================================================================
# Validasi options (entry point)
# ===========================================================================

@pytest.mark.parametrize(
    "bind",
    ["", "   ", "host with space", "127.0.0.1:8787", 123, None],
)
def test_options_validation_rejects_invalid_bind(bind: Any):
    with pytest.raises(ValueError):
        createAgentServer(AgentServerOptions(bind=bind))


@pytest.mark.parametrize("port", [-1, 70000, 1.5, True, None, "8080"])
def test_options_validation_rejects_invalid_port(port: Any):
    with pytest.raises(ValueError):
        createAgentServer(AgentServerOptions(port=port))


def test_options_validation_rejects_invalid_wasm_path():
    with pytest.raises(ValueError, match="wasm_path"):
        createAgentServer(AgentServerOptions(wasm_path=Path("definitely/missing.wasm")))


def test_options_validation_rejects_unknown_default_provider():
    with pytest.raises(ValueError, match="default_provider"):
        createAgentServer(AgentServerOptions(default_provider="openai"))


def test_options_validation_rejects_unknown_provider_spec_type():
    with pytest.raises(ValueError, match="unknown provider spec type"):
        createAgentServer(AgentServerOptions(providers={"stub": {"type": "openai"}}))


def test_options_validation_rejects_malformed_stub_spec():
    with pytest.raises(ValueError, match="response"):
        createAgentServer(AgentServerOptions(providers={"stub": {"type": "stub"}}))
    with pytest.raises(ValueError, match="not valid JSON"):
        createAgentServer(
            AgentServerOptions(providers={"stub": {"type": "stub", "response": "{bad"}})
        )


def test_options_validation_rejects_bad_scalar_fields():
    with pytest.raises(ValueError):
        createAgentServer(AgentServerOptions(client_id=""))
    with pytest.raises(ValueError):
        createAgentServer(AgentServerOptions(max_steps=-1))
    with pytest.raises(ValueError):
        createAgentServer(AgentServerOptions(max_steps=True))
    with pytest.raises(ValueError):
        createAgentServer(AgentServerOptions(corr_ttl_secs=0))
    with pytest.raises(ValueError):
        createAgentServer(AgentServerOptions(keepalive_secs=-5))
    with pytest.raises(TypeError):
        createAgentServer(AgentServerOptions(runtime_hooks_enabled="yes"))
    with pytest.raises(TypeError):
        createAgentServer(AgentServerOptions(providers=["stub"]))
    with pytest.raises(TypeError):
        createAgentServer(AgentServerOptions(policy="deny-all"))


def test_create_agent_server_rejects_non_options():
    with pytest.raises(TypeError):
        createAgentServer("nope")  # type: ignore[arg-type]


def test_create_agent_server_without_args_uses_defaults():
    server = createAgentServer()
    assert isinstance(server, AgentServer)


def test_create_agent_server_accepts_dict_options():
    """Kontrak facade: options boleh dict (dikonversi + divalidasi entry point)."""
    server = createAgentServer({"port": 0, "client_id": "dict-client"})
    assert isinstance(server, AgentServer)
    url = server.start()
    try:
        assert url.startswith("http://127.0.0.1:")
        # Dict options dipakai: client_id server = "dict-client".
        status, raw, _ = _request("GET", url + "/antikythera/v1/tools")
        assert status == 200
    finally:
        server.stop()


def test_create_agent_server_dict_options_rejects_unknown_keys():
    with pytest.raises(TypeError):
        createAgentServer({"port": 0, "bogus_key": 1})


def test_create_agent_server_dict_options_rejects_invalid_values():
    with pytest.raises((TypeError, ValueError)):
        createAgentServer({"port": 70000})
    with pytest.raises((TypeError, ValueError)):
        createAgentServer({"bind": ""})
    with pytest.raises(ValueError, match="wasm_path"):
        createAgentServer({"wasm_path": Path("definitely/missing.wasm")})


# ===========================================================================
# Komposisi default + properti
# ===========================================================================

def test_default_composition_properties():
    server = createAgentServer()
    assert isinstance(server.registry, UnionRegistry)
    assert isinstance(server.gate, PolicyGate)
    assert isinstance(server.control, ControlChannel)
    assert isinstance(server.transport, ThreadingHttpTransport)
    assert isinstance(server.component, ComponentServer)
    assert server.server_url is None
    # Default-deny (R4): gate kosong di ketiga destination.
    assert server.gate.allowed_tools("server") == frozenset()
    assert server.gate.allowed_tools("client") == frozenset()
    assert server.gate.allowed_tools("mcp") == frozenset()
    with pytest.raises(RuntimeError, match="not started"):
        server.url()


def test_policy_option_injected():
    policy = PolicyGate()
    policy.allow_tool("server", "pre_allowed")
    server = createAgentServer(AgentServerOptions(policy=policy))
    assert server.gate is policy


# ===========================================================================
# Lifecycle start/stop/url
# ===========================================================================

def test_start_returns_active_url_and_stop_closes():
    server = createAgentServer()
    url = server.start()
    assert url == server.url()
    assert server.server_url == url
    assert url.startswith("http://127.0.0.1:")
    port = int(url.rsplit(":", 1)[1])
    assert 0 < port < 65536
    # HTTP nyata aktif setelah start.
    status, raw, _ = _request("GET", url + "/antikythera/v1/tools")
    assert status == 200
    # Stop menutup socket: koneksi berikutnya ditolak.
    server.stop()
    assert server.server_url is None
    with pytest.raises(urllib.error.URLError):
        urllib.request.urlopen(url + "/antikythera/v1/tools", timeout=2)


def test_stop_is_idempotent_and_restartable():
    server = createAgentServer()
    url1 = server.start()
    assert url1.startswith("http://127.0.0.1:")
    server.stop()
    server.stop()  # idempotent
    url2 = server.start()
    assert url2.startswith("http://127.0.0.1:")
    status, raw, _ = _request("GET", url2 + "/antikythera/v1/tools")
    assert status == 200
    server.stop()


def test_double_start_raises():
    server = createAgentServer()
    server.start()
    try:
        with pytest.raises(RuntimeError, match="already started"):
            server.start()
    finally:
        server.stop()


# ===========================================================================
# Registrasi tool + POST /tools/execute nyata
# ===========================================================================

def test_register_server_tool_deny_then_allow():
    server = createAgentServer()
    server.register_server_tool(
        {"name": "echo_tool", "description": "Echo args"},
        handler=lambda args: {"echoed": args},
    )
    url = server.start()
    try:
        body = {
            "tool-name": "echo_tool",
            "arguments-json": "{}",
            "session-id": "session-123",
            "step-id": 1,
        }
        # Default-deny: registrasi BUKAN grant di facade — gate kosong → 403.
        status, raw, _ = _request("POST", url + "/antikythera/v1/tools/execute", body)
        assert status == 403
        parsed = _json(raw)
        assert parsed == {"error": "permission: tool 'echo_tool' not in allowlist"}
        assert set(parsed.keys()) == set(GOLDEN["error_event"]["payload"].keys())

        # Allowlist server → eksekusi sukses shape golden tool_execute_response.
        server.gate.allow_tool("server", "echo_tool")
        status, raw, _ = _request("POST", url + "/antikythera/v1/tools/execute", body)
        assert status == 200
        result = _json(raw)
        assert result == {
            "tool-name": "echo_tool",
            "success": True,
            "output-json": '{"echoed":{}}',
            "error-message": None,
            "step-id": 1,
        }
        assert set(result.keys()) == set(GOLDEN["tool_execute_response"].keys())
    finally:
        server.stop()


def test_register_client_tools():
    server = createAgentServer()
    server.register_client_tools(
        [
            {"name": "cli_a", "description": "client tool a"},
            {"name": "cli_b", "description": "client tool b"},
        ]
    )
    assert server.registry.owner_of("cli_a") == "client"
    assert server.registry.owner_of("cli_b") == "client"
    assert [d["name"] for d in server.registry.definitions()] == ["cli_a", "cli_b"]
    with pytest.raises(TypeError):
        server.register_client_tools("not-a-list")  # type: ignore[arg-type]
    with pytest.raises(ValueError):
        server.register_client_tools([{"name": "no_description"}])


def test_connect_mcp_server_raises_not_implemented():
    server = createAgentServer()
    with pytest.raises(NotImplementedError, match="MCP"):
        server.connect_mcp_server({"command": "npx", "args": ["-y", "server"]})


# ===========================================================================
# LLM proxy + manifest (core@client surface)
# ===========================================================================

def test_llm_call_default_stub_response():
    server = createAgentServer()
    url = server.start()
    try:
        status, raw, _ = _request("POST", url + "/antikythera/v1/llm/call", LLM_REQUEST_STUB_PROVIDER)
        assert status == 200
        body = _json(raw)
        assert set(body.keys()) == set(GOLDEN["llm_call_response"].keys())
        assert body["content"] == '{"content": "stub response", "finish_reason": "stop"}'
        assert body["finish_reason"] == "stop"
    finally:
        server.stop()


def test_llm_call_provider_dict_spec_override():
    server = createAgentServer(
        AgentServerOptions(
            providers={"stub": {"type": "stub", "response": '{"content":"custom-stub"}'}}
        )
    )
    url = server.start()
    try:
        status, raw, _ = _request("POST", url + "/antikythera/v1/llm/call", LLM_REQUEST_STUB_PROVIDER)
        assert status == 200
        body = _json(raw)
        assert body["content"] == '{"content":"custom-stub"}'
        assert set(body.keys()) == set(GOLDEN["llm_call_response"].keys())
    finally:
        server.stop()


def test_llm_call_unknown_provider_returns_400():
    server = createAgentServer()
    url = server.start()
    try:
        status, raw, _ = _request(
            "POST", url + "/antikythera/v1/llm/call", dict(GOLDEN["llm_call_request"], provider="nope")
        )
        assert status == 400
        assert _json(raw)["error"].startswith("unknown LLM provider:")
    finally:
        server.stop()


def test_component_manifest_returns_golden_shape():
    server = createAgentServer()
    url = server.start()
    try:
        status, raw, ctype = _request("GET", url + "/antikythera/v1/component/manifest")
        assert status == 200
        assert ctype == "application/json"
        manifest = _json(raw)
        assert manifest == GOLDEN["component_manifest"]
        assert set(manifest.keys()) == set(GOLDEN["component_manifest"].keys())
    finally:
        server.stop()


# ===========================================================================
# Loop core@server (D6; wasmtime + komposit paket)
# ===========================================================================

@pytest.fixture
def packaged_wasm() -> Path:
    pytest.importorskip(
        "wasmtime",
        reason="wasmtime not installed; install with: pip install antikythera-agent[wasm]",
    )
    if not _PACKAGED_WASM.is_file():
        pytest.skip(
            f"packaged composite not found at {_PACKAGED_WASM}; "
            "the loop core@server test cannot run without the composite"
        )
    return _PACKAGED_WASM


def test_run_server_loop_requires_wasm_path():
    server = createAgentServer()
    with pytest.raises(RuntimeError, match="wasm_path"):
        server.run_server_loop(ToolLoopConfig())


def test_run_server_loop_rejects_non_config():
    server = createAgentServer()
    with pytest.raises(TypeError):
        server.run_server_loop({"session_id": "x"})  # type: ignore[arg-type]


def test_run_server_loop_reaches_final(packaged_wasm: Path):
    server = createAgentServer(
        AgentServerOptions(
            wasm_path=packaged_wasm,
            runtime_hooks_enabled=False,
            providers={"stub": {"type": "stub", "response": '{"action":"final","content":"loop-ok"}'}},
        )
    )
    outcome = server.run_server_loop(
        ToolLoopConfig(session_id="t-loop", prompts=["hello"], provider="stub")
    )
    assert outcome.action == "final"
    assert outcome.content == "loop-ok"
    assert outcome.steps == 1


def test_run_server_loop_forwards_runtime_hooks_enabled(packaged_wasm: Path):
    # Default AgentServerOptions.runtime_hooks_enabled=True DITERUSKAN ke loop:
    # tanpa allowlist hook, denial gate `permission: hook` meluas (bukan
    # passthrough senyap) — membuktikan server option dikonsultasikan.
    server = createAgentServer(AgentServerOptions(wasm_path=packaged_wasm))
    with pytest.raises(Exception) as exc_info:
        server.run_server_loop(ToolLoopConfig(session_id="t-hooks", prompts=["hello"]))
    assert "permission: hook" in str(exc_info.value)


# ===========================================================================
# CLI spawn (U61 parity): listening line + HTTP nyata
# ===========================================================================

def test_cli_spawn_serves_llm_call_golden_shape():
    port = _free_port()
    stub = {"action": "final", "content": "cli-ok"}
    cli = start_cli(["--provider-stub", json.dumps(stub)], port)
    try:
        assert cli.listening_line.strip().startswith("[server-runtime] HTTP wire bridge listening on ")
        status, raw, _ = _request(
            "POST",
            cli.url + "/antikythera/v1/llm/call",
            {
                "provider": None,
                "model": "stub",
                "session_id": "s-cli",
                "messages_json": "[]",
                "force_json": False,
                "temperature": None,
                "max_tokens": None,
                "schema_name": None,
                "metadata_json": None,
            },
        )
        assert status == 200
        body = _json(raw)
        assert set(body.keys()) == set(GOLDEN["llm_call_response"].keys())
        assert body["content"] == '{"action": "final", "content": "cli-ok"}'
    finally:
        cli.stop()


def test_cli_server_tool_registers_and_executes():
    port = _free_port()
    cli = start_cli(["--server-tool", 'server_echo:{"ok":true}'], port)
    try:
        # GET /tools: definisi tool terdaftar (parity 2d).
        status, raw, _ = _request("GET", cli.url + "/antikythera/v1/tools")
        assert status == 200
        defs = _json(raw)
        echo = next(d for d in defs if d["name"] == "server_echo")
        assert echo["description"] == "Server tool registered via --server-tool"
        assert echo["input_schema"] == {"type": "object", "properties": {}, "required": []}

        # POST /tools/execute: registrasi = grant → sukses tanpa --allow-tool.
        status, raw, _ = _request(
            "POST",
            cli.url + "/antikythera/v1/tools/execute",
            {"tool-name": "server_echo", "arguments-json": "{}", "session-id": "s", "step-id": 1},
        )
        assert status == 200
        result = _json(raw)
        assert result["success"] is True
        assert json.loads(result["output-json"]) == {"ok": True}
        assert set(result.keys()) == set(GOLDEN["tool_execute_response"].keys())
    finally:
        cli.stop()


def test_cli_server_tool_response_json_may_contain_colons():
    # Nama = teks sebelum titik dua PERTAMA (ServerToolSpec::parse).
    port = _free_port()
    cli = start_cli(["--server-tool", 'deep:{"a:b":{"b":1}}'], port)
    try:
        status, raw, _ = _request(
            "POST",
            cli.url + "/antikythera/v1/tools/execute",
            {"tool-name": "deep", "arguments-json": "{}", "session-id": "s", "step-id": 0},
        )
        assert status == 200
        result = _json(raw)
        assert json.loads(result["output-json"]) == {"a:b": {"b": 1}}
    finally:
        cli.stop()


def test_cli_allow_tool_without_registration_fails_closed():
    # Mirror parity 2b: --allow-tool mengubah policy, BUKAN registri; tanpa
    # registrasi, POST /tools/execute tetap fail-closed 403 `permission:`.
    port = _free_port()
    cli = start_cli(["--allow-tool", "srv_echo"], port)
    try:
        status, raw, _ = _request(
            "POST",
            cli.url + "/antikythera/v1/tools/execute",
            {"tool-name": "srv_echo", "arguments-json": "{}", "session-id": "s", "step-id": 1},
        )
        assert status == 403
        parsed = _json(raw)
        assert parsed == {"error": "permission: tool 'srv_echo' not in allowlist"}
        assert set(parsed.keys()) == set(GOLDEN["error_event"]["payload"].keys())
    finally:
        cli.stop()


def test_cli_invalid_flag_errors_and_exits():
    proc = subprocess.Popen(
        [
            sys.executable,
            "-m",
            "antikythera_agent.server",
            "--bind",
            "not-an-addr",
            "--provider-stub",
            "{bad json",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env={**os.environ, "PYTHONPATH": str(_PYTHON_SRC)},
        cwd=str(_REPO_ROOT),
        text=True,
    )
    out, err = proc.communicate(timeout=20)
    assert proc.returncode == 2
    assert "error:" in err
    assert "listening on" not in out
