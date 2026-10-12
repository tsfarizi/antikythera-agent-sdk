import json
import threading
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

from antikythera_agent.runtime import WasmRuntime
from antikythera_agent.orchestrator import Orchestrator
from antikythera_agent.types import AgentProfileConfig
from antikythera_agent.server import provider
from antikythera_agent.server.bridge import createAgentServer


def test_wasmtime_hooks_are_owned_by_each_component_instance():
    class Hooks:
        def __init__(self, label):
            self.label = label

        def decide_action(self, state, response):
            return json.dumps({"action": "final", "content": self.label})

    a, b = WasmRuntime(hooks_provider=Hooks("A")), WasmRuntime(hooks_provider=Hooks("B"))
    for runtime, label in [(a, "A"), (b, "B"), (a, "A")]:
        sid = runtime.call("init", '{"session_id":"same","runtime_hooks_enabled":true}')
        prepared = runtime.call(
            "prepare_user_turn", json.dumps({"session_id": sid, "prompt": "hello"})
        )
        result = runtime.call(
            "commit_llm_response", json.dumps([prepared, '{"action":"final","content":"provider"}'])
        )
        assert json.loads(result)["content"] == label


def test_budget_reservations_and_cooperative_cancellation(monkeypatch):
    started, release = threading.Event(), threading.Event()
    calls = []

    class BlockingProvider(provider.StubProvider):
        def _call(self, request):
            calls.append(request)
            started.set()
            assert release.wait(10)
            return super()._call(request)

    monkeypatch.setitem(
        provider.provider_registry, "stub", BlockingProvider('{"action":"final","content":"ok"}')
    )
    orch = Orchestrator(execution_mode="concurrent", max_total_tasks=1, max_total_steps=1)
    orch.register_agent(AgentProfileConfig(id="a", name="A", role="worker", max_steps=3))
    with ThreadPoolExecutor(max_workers=2) as pool:
        running = pool.submit(orch.dispatch, "first", "known-session")
        assert started.wait(10)
        denied = orch.dispatch("second")
        assert not denied.success
        assert orch.cancel("known-session")
        release.set()
        result = running.result(timeout=10)
    assert not result.success and "cancelled" in result.error
    assert result.steps_used == 1 and len(calls) == 1
    assert orch.get_budget()["consumed_steps"] == 1


def test_http_auth_provider_streams_and_mcp_registration():
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            if self.path == "/mcp":
                if body["method"] == "notifications/initialized":
                    self.send_response(202)
                    self.end_headers()
                    return
                result = {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "serverInfo": {"name": "mock", "version": "1"},
                }
                if body["method"] == "tools/list":
                    result = {"tools": [{"name": "mcp_echo", "inputSchema": {"type": "object"}}]}
                if body["method"] == "tools/call":
                    result = {"structuredContent": body["params"]["arguments"]}
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(
                    json.dumps({"jsonrpc": "2.0", "id": body["id"], "result": result}).encode()
                )
                return
            self.send_response(200)
            self.end_headers()
            if self.path == "/api/chat":
                wire = '{"message":{"content":"é漢"},"done":false}\n{"done":true}\n'.encode()
            else:
                assert body["response_format"] == {"type": "json_object"}
                wire = (
                    'data: {"choices":[{"delta":{"content":"é漢"}}]}\n\ndata: [DONE]\n\n'.encode()
                )
            for byte in wire:
                self.wfile.write(bytes([byte]))
                self.wfile.flush()

    http = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=http.serve_forever, daemon=True)
    thread.start()
    endpoint = f"http://127.0.0.1:{http.server_port}"
    server = createAgentServer({"auth_token": "test-token"})
    try:
        request = {"model": "m", "messages_json": "[]", "force_json": True}
        for impl in [
            provider.OllamaProvider(endpoint, model="m"),
            provider.OpenAiProvider(endpoint, "key", "m"),
        ]:
            tokens = []
            assert impl.call_stream(request, tokens.append)["content"] == "é漢"
            assert tokens == ["é漢"]
        server.connect_mcp_server({"url": endpoint + "/mcp"})
        server._gate.allow_tool("mcp", "mcp_echo")
        status, result = server._transport.handle_tools_execute(
            {"tool-name": "mcp_echo", "arguments-json": '{"value":7}', "step-id": 1}
        )
        assert status == 200 and result["success"]
        assert json.loads(result["output-json"]) == {"value": 7}
        url = server.start()
        import urllib.request
        import urllib.error

        with pytest.raises(urllib.error.HTTPError) as denied:
            urllib.request.urlopen(url + "/antikythera/v1/tools")
        assert denied.value.code == 401
        req = urllib.request.Request(
            url + "/antikythera/v1/tools", headers={"Authorization": "Bearer test-token"}
        )
        with urllib.request.urlopen(req) as response:
            assert response.status == 200
    finally:
        server.stop()
        http.shutdown()
        http.server_close()
        thread.join(timeout=5)


def test_public_bind_requires_authentication():
    with pytest.raises(ValueError, match="auth_token"):
        createAgentServer({"bind": "0.0.0.0"})


def test_stdio_mcp_client_and_reconnect_generation():
    import sys
    from antikythera_agent.server.mcp import McpClient
    from antikythera_agent.server.control import ControlChannel

    script = """
import json, sys
for line in sys.stdin:
    message = json.loads(line)
    if 'id' not in message: continue
    result = {'protocolVersion':'2025-06-18'}
    if message['method'] == 'tools/list': result = {'tools':[{'name':'echo'}]}
    if message['method'] == 'tools/call': result = {'structuredContent':message['params']['arguments']}
    print(json.dumps({'jsonrpc':'2.0','id':message['id'],'result':result}), flush=True)
"""
    client = McpClient({"command": sys.executable, "args": ["-u", "-c", script]})
    try:
        assert client.tools() == [{"name": "echo"}]
        assert client.call_tool("echo", {"value": 4}) == {"value": 4}
    finally:
        client.close()
    control = ControlChannel()
    try:

        def old(frame):
            pass

        def new(frame):
            pass

        control.register_client("peer", old)
        control.register_client("peer", new)
        assert not control.unregister_client("peer", old)
        assert control.is_client_connected("peer")
        assert control.unregister_client("peer", new)
    finally:
        control.close()


def test_llm_quota_capacity_does_not_reset_existing_sessions():
    from antikythera_agent.server.gate import PolicyGate, PermissionDeniedError

    gate = PolicyGate(llm_quota=1)
    for index in range(1024):
        gate.check_llm(f"session-{index}")
    with pytest.raises(PermissionDeniedError, match="capacity"):
        gate.check_llm("new-session")
    with pytest.raises(PermissionDeniedError, match="quota"):
        gate.check_llm("session-0")
