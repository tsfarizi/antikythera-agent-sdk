"""Native Component Model acceptance checks for configured-agent delivery."""

import hashlib
import json
import urllib.error
import urllib.request

import pytest

from antikythera_agent import Agent, AgentDeployment
from antikythera_agent.server.provider import LlmProvider
from antikythera_agent.tools import tool


class ReceiptProvider(LlmProvider):
    """Return final content only after the real handler's output reaches message history."""

    def __init__(self):
        self.requests = []

    def _call(self, request):
        self.requests.append(request)
        messages = json.loads(request["messages_json"])
        tool_messages = [message for message in messages if message["role"] == "tool"]
        if tool_messages:
            assert '"receipt": 7' in tool_messages[-1]["content"]
            return {"content": '{"action":"final","content":"delivered-7"}'}
        return {"content": '{"action":"call_tool","tool":"delivery_receipt","input":{"value":7}}'}


def configured_agent(allow=True, hooks=None):
    """Build the same configured agent used by the Python-to-jco acceptance path."""
    provider = ReceiptProvider()
    calls = []

    @tool(name="delivery_receipt", description="Return a numeric receipt")
    def receipt(args):
        """Record actual execution and return a marker consumed by the next model request."""
        calls.append(args)
        return {"receipt": args["value"]}

    agent = Agent(
        provider="receipt",
        model="configured-model",
        system_prompt="Shared agent prompt",
        max_steps=4,
        provider_resolver=lambda name: provider,
        hooks_provider=hooks,
    )
    agent.register_tool(receipt, allow=allow)
    return agent, provider, calls


def post(url, body, token=None):
    """Issue an actual JSON HTTP request to the deployment host."""
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, json.dumps(body).encode(), headers, method="POST")
    with urllib.request.urlopen(request, timeout=10) as response:
        return json.load(response)


def test_existing_agent_configuration_executes_with_preserved_tool_and_provider_bindings():
    agent, provider, calls = configured_agent()
    deployment = agent.deployment(agent_id="receipt-agent")
    result = deployment.run("hello")
    assert result.success and result.output == "delivered-7"
    assert result.steps_used == 2 and calls == [{"value": 7}]
    assert provider.requests[0]["model"] == "configured-model"
    assert "Shared agent prompt" in provider.requests[0]["messages_json"]


def test_unallowed_registered_tools_are_denied_before_handler_execution():
    agent, _, calls = configured_agent(allow=False)
    deployment = AgentDeployment.from_agent(agent, agent_id="restricted")
    result = deployment.run("hello")
    assert not result.success and "permission:" in result.error
    assert calls == []
    with pytest.raises(PermissionError):
        deployment.execute_tool({"tool-name": "delivery_receipt"})


def test_export_contains_matching_wasm_and_jco_assets_with_a_complete_hash_inventory(tmp_path):
    agent, _, _ = configured_agent()
    deployment = agent.deployment(agent_id="receipt-agent")
    destination = tmp_path / "bundle"
    manifest = deployment.export_client(destination, server_url="http://127.0.0.1:9000")
    assert (destination / "agent.wasm").read_bytes()[:8] == b"\x00asm\x0d\x00\x01\x00"
    assert (destination / "component/factory/antikythera-sdk.js").is_file()
    for entry in manifest["files"]:
        data = (destination / entry["path"]).read_bytes()
        assert len(data) == entry["size"]
        assert hashlib.sha256(data).hexdigest() == entry["sha256"]
    definition = json.loads((destination / "agent.json").read_text())
    assert definition["agent"]["model"] == "configured-model"
    assert definition["agent"]["tools"][0]["allow"] is True
    assert "provider_resolver" not in (destination / "agent.json").read_text()


def test_export_requires_explicit_hook_bindings_and_preserves_existing_user_files(tmp_path):
    class Hooks:
        """A Python callback requires a corresponding JS module for client execution."""

        def decide_action(self, state, response):
            return '{"passthrough":true}'

    agent, _, _ = configured_agent(hooks=Hooks())
    deployment = agent.deployment(agent_id="hooked")
    destination = tmp_path / "bundle"
    with pytest.raises(ValueError, match="hooks_module"):
        deployment.export_client(destination)
    assert not destination.exists()
    bindings = tmp_path / "bindings"
    bindings.mkdir()
    (bindings / "hooks.mjs").write_text("export default {decideAction:()=>({passthrough:true})};")
    deployment.export_client(destination, bindings_dir=bindings, hooks_module="hooks.mjs")
    before = (destination / "agent.json").read_bytes()
    with pytest.raises(ValueError, match="already exists"):
        deployment.export_client(destination, bindings_dir=bindings, hooks_module="hooks.mjs")
    assert (destination / "agent.json").read_bytes() == before


def test_client_tools_require_registered_names_and_safe_module_paths(tmp_path):
    agent, _, _ = configured_agent()
    deployment = agent.deployment(agent_id="receipt-agent")
    destination = tmp_path / "bundle"
    with pytest.raises(ValueError, match="unknown client tool"):
        deployment.export_client(destination, client_tools={"unknown": "tool.mjs"})
    with pytest.raises(ValueError, match="relative"):
        deployment.export_client(destination, client_tools={"delivery_receipt": "../tool.mjs"})
    with pytest.raises(ValueError, match="credentials"):
        deployment.export_client(destination, server_url="http://user:secret@example.com")
    assert not destination.exists()


def test_python_hook_environment_runs_in_the_native_component():
    class Hooks:
        """Override the final response through the owning Wasmtime instance."""

        def decide_action(self, state, response):
            return '{"action":"final","content":"hook-result"}'

    agent, _, calls = configured_agent(hooks=Hooks())
    result = agent.deployment(agent_id="hooked").run("hello")
    assert result.success and result.output == "hook-result"
    assert calls == []


def test_http_delivery_runs_native_sessions_serves_assets_and_keeps_auth_out_of_manifest():
    agent, provider, calls = configured_agent()
    deployment = agent.deployment(agent_id="receipt-agent")
    with deployment.serve(auth_token="host-secret") as host:
        with urllib.request.urlopen(host.client_url) as response:
            assert response.headers["Content-Type"] == "text/javascript"
        with urllib.request.urlopen(host.url + "/bundle/agent.wasm") as response:
            assert response.headers["Content-Type"] == "application/wasm"
        with urllib.request.urlopen(host.url + "/bundle/agent.json") as response:
            assert b"host-secret" not in response.read()
        with pytest.raises(urllib.error.HTTPError) as denied:
            post(host.url + "/run", {"prompt": "hello"})
        assert denied.value.code == 401
        for _ in range(4):
            with pytest.raises(urllib.error.HTTPError) as large_denied:
                post(host.url + "/run", {"prompt": "x" * (128 * 1024)})
            assert large_denied.value.code == 401
        assert calls == [] and provider.requests == []
        result = post(host.url + "/run", {"prompt": "hello"}, "host-secret")
        assert result["output"] == "delivered-7" and result["stepsUsed"] == 2
        second = post(host.url + "/run", {"prompt": "hello"}, "host-secret")
        assert second["sessionId"] != result["sessionId"]
        post(host.url + "/llm/call", {"messages_json": "[]", "model": "attacker"}, "host-secret")
        assert provider.requests[-1]["model"] == "configured-model"
        assert len(calls) == 2
        with pytest.raises(urllib.error.HTTPError) as forbidden:
            post(host.url + "/tools/execute", {"tool-name": "unknown"}, "host-secret")
        assert forbidden.value.code == 403
        with pytest.raises(urllib.error.HTTPError) as invalid:
            post(host.url + "/run", {"prompt": []}, "host-secret")
        assert invalid.value.code == 400


def test_delivery_hosts_require_auth_when_binding_beyond_loopback():
    agent, _, _ = configured_agent()
    with pytest.raises(ValueError, match="auth_token"):
        agent.deployment(agent_id="receipt-agent").serve(bind="0.0.0.0")
