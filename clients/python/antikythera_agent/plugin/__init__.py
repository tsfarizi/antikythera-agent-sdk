"""
antikythera_agent.plugin — Plugin surface (optional, wasm-heavy).

Re-export shim for the new namespace. Contains the WASM runtime and
related heavy dependencies that are optional via `pip install antikythera-agent[wasm]`.
"""

from antikythera_agent.runtime import WasmRuntime, WasmRuntimeError

__all__ = [
    "WasmRuntime",
    "WasmRuntimeError",
]
