"""Python sugar layer: class/function-based tool registration (T3).

Re-export facade T3:
- `BaseTool` + `DEFAULT_INPUT_SCHEMA` dari base
- `tool` decorator dari decorator
- `register_tool`, `is_tool`, `extract_tool_entry` dari helpers

Invarian T3:
- shape golden 6-key `{name,title,description,parameters,input_schema,output_schema}`
  tanpa field ekstra (owner terpisah); deepCopy simpan+return; sort oleh registry.
- collision message kanonik existing-first dijaga registry.
- register != allow strict; nol global auto-registry on import.
- import `antikythera_agent.tools` tanpa wasmtime harus sukses (zero-dependency).
"""

from .base import DEFAULT_INPUT_SCHEMA, BaseTool
from .decorator import tool
from .helpers import extract_tool_entry, is_tool, register_tool

__all__ = [
    "BaseTool",
    "DEFAULT_INPUT_SCHEMA",
    "tool",
    "register_tool",
    "is_tool",
    "extract_tool_entry",
]
