"""
antikythera_agent.core — Core runtime surface (stable, no optional wasm).

Re-export shim for the new namespace. The underlying modules remain in
`antikythera_agent.*`; this package provides the `antikythera.core` entry
point per the namespace contract (core does not import plugin).
"""

from antikythera_agent.agent import Agent
from antikythera_agent.orchestrator import Orchestrator
from antikythera_agent.prompts import PromptConfig, PromptManager
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
    "Agent",
    "Orchestrator",
    "SessionManager",
    "PromptManager",
    "PromptConfig",
    "AgentConfig",
    "AgentResult",
    "AgentProfileConfig",
    "TaskResult",
    "PipelineResult",
    "OrchestratorConfig",
    "SessionInfo",
    "get_version",
]
