"""Type definitions for Antikythera SDK."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Literal, Optional


@dataclass
class AgentConfig:
    """Configuration for creating an Agent instance.

    Attributes:
        provider: The LLM provider identifier (e.g., 'openai', 'anthropic', 'gemini').
        model: The model name to use (e.g., 'gpt-4o', 'claude-3-opus').
        system_prompt: Optional system prompt override.
        max_steps: Maximum reasoning steps before forced completion.
        timeout: Request timeout in milliseconds.
    """

    provider: str
    model: str
    system_prompt: Optional[str] = None
    max_steps: int = 8
    timeout: int = 60000


@dataclass
class AgentResult:
    """Result from an agent execution.

    Attributes:
        output: The agent's response text.
        success: Whether the execution completed successfully.
        steps_used: Number of reasoning steps taken.
        session_id: The session identifier for this conversation.
        error: Error message if execution failed.
    """

    output: str
    success: bool
    steps_used: int
    session_id: str
    error: Optional[str] = None


@dataclass
class AgentProfileConfig:
    """Configuration for an agent profile in multi-agent orchestration.

    Attributes:
        id: Unique identifier for this agent.
        name: Human-readable display name.
        role: Semantic role label (e.g., 'coder', 'reviewer', 'analyst').
        system_prompt: System prompt defining this agent's behavior.
        max_steps: Maximum reasoning steps for this agent.
    """

    id: str
    name: str
    role: str
    system_prompt: Optional[str] = None
    max_steps: int = 8


@dataclass
class TaskResult:
    """Result from a single task in multi-agent orchestration.

    Attributes:
        task_id: Unique identifier for the task.
        agent_id: ID of the agent that executed the task.
        output: The task output as a JSON-serializable value.
        success: Whether the task completed successfully.
        steps_used: Number of reasoning steps taken.
        session_id: Session identifier.
        error: Error message if task failed.
        error_kind: Error classification ('transient', 'permanent', 'cancelled').
        duration_ms: Execution time in milliseconds.
    """

    task_id: str
    agent_id: str
    output: Any
    success: bool
    steps_used: int
    session_id: str
    error: Optional[str] = None
    error_kind: Optional[Literal["transient", "permanent", "cancelled"]] = None
    duration_ms: int = 0


@dataclass
class PipelineResult:
    """Result from a pipeline of sequential tasks.

    Attributes:
        results: Individual task results.
        final_output: The final output from the last task.
        total_steps: Total steps across all tasks.
        success: Whether all tasks completed successfully.
        error: Error message if any task failed.
    """

    results: list[TaskResult]
    final_output: Any
    total_steps: int
    success: bool
    error: Optional[str] = None


@dataclass
class OrchestratorConfig:
    """Configuration for the multi-agent orchestrator.

    Attributes:
        execution_mode: How tasks are executed.
        max_concurrent_tasks: Maximum tasks running simultaneously.
        max_total_steps: Maximum total steps across all tasks.
        max_total_tasks: Maximum number of tasks.
        default_retry_condition: Retry policy.
    """

    execution_mode: Literal["auto", "sequential", "concurrent", "parallel"] = "auto"
    max_concurrent_tasks: int = 4
    max_total_steps: Optional[int] = None
    max_total_tasks: Optional[int] = None
    default_retry_condition: Literal["always", "on-transient", "never"] = "always"


@dataclass
class SessionInfo:
    """Session information.

    Attributes:
        session_id: Unique session identifier.
        agent_id: Agent associated with this session.
        created_at: Creation timestamp (Unix milliseconds).
        last_activity: Last activity timestamp (Unix milliseconds).
        message_count: Number of messages in this session.
    """

    session_id: str
    agent_id: str
    created_at: int
    last_activity: int
    message_count: int
