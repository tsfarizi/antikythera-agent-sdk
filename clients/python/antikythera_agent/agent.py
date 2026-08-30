"""Agent implementation."""

from __future__ import annotations

from typing import Any, Callable, List, Optional, Tuple

from antikythera_agent.local_loop import LocalLoopConfig, ToolLoopError, run_local_loop
from antikythera_agent.runtime import WasmRuntime, WasmRuntimeError
from antikythera_agent.server.gate import PermissionDeniedError
from antikythera_agent.types import AgentConfig, AgentResult


class Agent:
    """Single agent execution runtime.

    Creates and runs an agent with a specific provider and model configuration.
    Handles prompt processing, tool calls, and response generation through the
    local tool loop engine (`antikythera_agent.local_loop`).

    Example:
        >>> agent = Agent(provider="openai", model="gpt-4o")
        >>> result = agent.run("Explain quantum computing")
        >>> print(result.output)
    """

    def __init__(
        self,
        provider: str,
        model: str,
        system_prompt: Optional[str] = None,
        max_steps: int = 8,
        timeout: int = 60000,
        session_id: Optional[str] = None,
        provider_resolver: Optional[Callable[[Optional[str]], Any]] = None,
    ):
        """Create a new Agent instance.

        Args:
            provider: The LLM provider identifier.
            model: The model name to use.
            system_prompt: Optional system prompt override.
            max_steps: Maximum reasoning steps.
            timeout: Request timeout in milliseconds.
            session_id: Optional session identifier for continuity; when
                omitted, the runner creates one and it is reused across runs.
            provider_resolver: Optional resolver `(name) -> LlmProvider`;
                defaults to `server.provider.resolve_provider`.
        """
        if not isinstance(provider, str) or not provider:
            raise ValueError("provider must be a non-empty string")
        if not isinstance(model, str):
            raise TypeError("model must be a string")
        if not model:
            raise ValueError("model must be a non-empty string")
        if not isinstance(max_steps, int) or isinstance(max_steps, bool) or max_steps < 0:
            raise ValueError("max_steps must be a non-negative integer")
        if not isinstance(timeout, int) or isinstance(timeout, bool) or timeout < 0:
            raise ValueError("timeout must be a non-negative integer")

        self._config = AgentConfig(
            provider=provider,
            model=model,
            system_prompt=system_prompt,
            max_steps=max_steps,
            timeout=timeout,
        )
        # `AgentConfig` has no session field (frozen types contract); the
        # instance-level session lives here and is refreshed after each
        # successful run so consecutive runs continue the same session.
        self._session_id = session_id
        self._provider_resolver = provider_resolver
        self._runtime = WasmRuntime()
        self._pending_tools: List[Tuple[Any, bool]] = []

    @classmethod
    def from_config(cls, config: AgentConfig) -> Agent:
        """Create an Agent from a config object.

        Args:
            config: Agent configuration.

        Returns:
            Configured Agent instance.
        """
        return cls(
            provider=config.provider,
            model=config.model,
            system_prompt=config.system_prompt,
            max_steps=config.max_steps,
            timeout=config.timeout,
        )

    def run(self, prompt: str, session_id: Optional[str] = None) -> AgentResult:
        """Run the agent with a user prompt.

        Args:
            prompt: The user's input prompt.
            session_id: Optional session ID override for this run.

        Returns:
            Agent execution result. Infallible per S6: loop/LLM/tool failures
            surface as `success=False` with `error`, never as an exception.

        Raises:
            TypeError: If `prompt` is not a string (program error pre-loop).
            ValueError: If `prompt` is empty (program error pre-loop).
        """
        if not isinstance(prompt, str):
            raise TypeError("prompt must be a string")
        if not prompt:
            raise ValueError("prompt must be a non-empty string")

        effective_session_id = (
            session_id if session_id is not None else self._session_id
        )
        loop_config = LocalLoopConfig(
            session_id=effective_session_id or "",
            max_steps=self._config.max_steps,
            provider=self._config.provider,
            model=self._config.model,
            system_prompt=self._config.system_prompt,
            timeout=self._config.timeout,
            prompts=[prompt],
        )
        registry = None
        gate = None
        if self._pending_tools:
            from antikythera_agent.server.gate import PolicyGate
            from antikythera_agent.server.registry import UnionRegistry
            from antikythera_agent.tools.helpers import register_tool as _register_tool

            registry = UnionRegistry()
            gate = PolicyGate()
            for _obj, _allow in list(self._pending_tools):
                _register_tool(registry, gate, _obj, allow=_allow)
        try:
            outcome = run_local_loop(
                self._runtime,
                self._provider_resolver,
                registry,
                gate,
                loop_config,
            )
        except (ToolLoopError, WasmRuntimeError, PermissionDeniedError) as e:
            return AgentResult(
                output="",
                success=False,
                steps_used=0,
                session_id=effective_session_id or "",
                error=str(e),
            )
        self._session_id = outcome.session_id
        return AgentResult(
            output=outcome.content or "",
            success=True,
            steps_used=outcome.steps,
            session_id=outcome.session_id,
            error=None,
        )

    def get_config(self) -> AgentConfig:
        """Get the agent's configuration.

        Returns:
            The agent configuration.
        """
        return AgentConfig(
            provider=self._config.provider,
            model=self._config.model,
            system_prompt=self._config.system_prompt,
            max_steps=self._config.max_steps,
            timeout=self._config.timeout,
        )

    def register_tool(self, obj: Any, allow: bool = False) -> None:
        """Daftarkan Tool sugar untuk local loop (BaseTool subclass atau @tool function).

        Tool disimpan di `self._pending_tools` dan di-inject sebagai ephemeral
        `UnionRegistry`+`PolicyGate` pada `run` berikutnya (register != allow
        strict; collision existing-first dijaga registry; 6-key golden di-
        validasi `helpers`). `allow=True` sekalian allowlist di gate ephemeral,
        `allow=False` hanya register tanpa allow.

        Raises:
            TypeError: bila obj bukan Tool atau allow bukan bool.
            ValueError: owner/keys invalid.
        """
        if not isinstance(allow, bool):
            raise TypeError("register_tool: 'allow' must be a bool")
        from antikythera_agent.tools.helpers import is_tool

        if not is_tool(obj):
            raise TypeError(f"object {obj!r} is not a Tool (missing @tool or BaseTool)")
        self._pending_tools.append((obj, allow))

    def register_tools(self, objs: Any, allow: bool = False) -> None:
        """Bulk helper untuk `register_tool` (loop; allow berlaku untuk semua)."""
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
