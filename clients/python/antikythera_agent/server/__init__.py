"""Server-side runtime bridge package (unit E03, WIRE_PROTOCOL; facade U32).

Ekspor publik:
- `createAgentServer` / `AgentServer` / `AgentServerOptions` — facade server
  runtime bridge (unit U32).
- Re-export unit yang wajar agar konsumen facade tidak perlu mengimpor modul
  internal: gate/registry/provider/control/component/transport/loop_owner/
  wire.
"""

from . import wire
from .bridge import AgentServer, AgentServerOptions, createAgentServer
from .component import ComponentServer
from .control import ControlChannel, PendingTimeoutError
from .gate import PermissionDeniedError, PolicyGate
from .loop_owner import LoopOutcome, ToolLoopConfig, ToolLoopError, run_tool_loop
from .provider import LlmError, LlmProvider, OllamaProvider, OpenAiProvider, StubProvider
from .registry import UnionRegistry
from .transport import ThreadingHttpTransport, Transport

__all__ = [
    "AgentServer",
    "AgentServerOptions",
    "ComponentServer",
    "ControlChannel",
    "LlmError",
    "LlmProvider",
    "LoopOutcome",
    "OllamaProvider",
    "OpenAiProvider",
    "PendingTimeoutError",
    "PermissionDeniedError",
    "PolicyGate",
    "StubProvider",
    "ThreadingHttpTransport",
    "ToolLoopConfig",
    "ToolLoopError",
    "Transport",
    "UnionRegistry",
    "createAgentServer",
    "run_tool_loop",
    "wire",
]
