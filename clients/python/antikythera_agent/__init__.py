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

from antikythera_agent.agent import Agent
from antikythera_agent.orchestrator import Orchestrator
from antikythera_agent.prompts import PromptConfig, PromptManager
from antikythera_agent.runtime import WasmRuntime, WasmRuntimeError
from antikythera_agent.session import SessionManager
from antikythera_agent.types import (
    AgentConfig,
    AgentProfileConfig,
    AgentResult,
    OrchestratorConfig,
    PipelineResult,
    SessionInfo,
    TaskResult,
)
from antikythera_agent.utils import get_version

__version__ = "1.8.5"
__all__ = [
    # Core classes
    "Agent",
    "Orchestrator",
    "SessionManager",
    "PromptManager",
    "WasmRuntime",
    # Types
    "AgentConfig",
    "AgentResult",
    "AgentProfileConfig",
    "TaskResult",
    "PipelineResult",
    "OrchestratorConfig",
    "SessionInfo",
    "PromptConfig",
    # Errors
    "WasmRuntimeError",
    # Utilities
    "get_version",
]
