"""Run a configured Python deployment for real Node/browser delivery acceptance tests."""

import json
import sys
from pathlib import Path

from antikythera_agent import Agent
from antikythera_agent.server.provider import LlmProvider
from antikythera_agent.tools import tool


class ReceiptProvider(LlmProvider):
    """Request the registered tool, then confirm its result from component-owned message history."""

    def _call(self, request):
        if any(message["role"] == "tool" for message in json.loads(request["messages_json"])):
            return {"content": '{"action":"final","content":"delivered-7"}'}
        return {"content": '{"action":"call_tool","tool":"delivery_receipt","input":{"value":7}}'}


@tool(name="delivery_receipt", description="Return a receipt for the supplied numeric value")
def echo(args):
    """Return a marker that must survive tool execution and the next prepared LLM request."""
    return {"receipt": args["value"]}


agent = Agent(
    provider="receipt",
    model="configured-model",
    system_prompt="Shared agent prompt",
    provider_resolver=lambda name: ReceiptProvider(),
)
agent.register_tool(echo, allow=True)
deployment = agent.deployment(agent_id="receipt-agent")
with deployment.serve() as server:
    deployment.export_client(Path(sys.argv[1]), server_url=server.url)
    print(json.dumps({"url": server.url, "clientUrl": server.client_url}), flush=True)
    sys.stdin.read()
