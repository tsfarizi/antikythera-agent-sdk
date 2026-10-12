"""
Antikythera Agent SDK

General-purpose agent runtime with multi-agent orchestration,
MCP tool integration, and WebAssembly support.

Example:
    >>> from antikythera_agent import Agent, PromptManager, PromptConfig
    >>> prompts = PromptManager()
    >>> prompts.register(PromptConfig(
    ...     id="assistant",
    ...     name="Assistant",
    ...     content="You are a helpful assistant.",
    ... ))
    >>> agent = Agent(
    ...     provider="openai",
    ...     model="gpt-4o",
    ...     system_prompt=prompts.get_content("assistant"),
    ... )
    >>> result = agent.run("Hello, how can you help me?")
    >>> print(result.output)
"""

from antikythera_agent.core import (
    Agent,
    AgentConfig,
    AgentProfileConfig,
    AgentResult,
    Orchestrator,
    OrchestratorConfig,
    PipelineResult,
    PromptConfig,
    PromptManager,
    SessionInfo,
    SessionManager,
    TaskResult,
    get_version,
)
from antikythera_agent.delivery import AgentDeployment, DeliveryServer
from antikythera_agent.plugin import WasmRuntime, WasmRuntimeError

__version__ = "1.8.5"
__all__ = [
    "Agent",
    "AgentDeployment",
    "DeliveryServer",
    "Orchestrator",
    "SessionManager",
    "PromptManager",
    "WasmRuntime",
    "AgentConfig",
    "AgentResult",
    "AgentProfileConfig",
    "TaskResult",
    "PipelineResult",
    "OrchestratorConfig",
    "SessionInfo",
    "PromptConfig",
    "WasmRuntimeError",
    "get_version",
]
