"""WASM runtime for executing antikythera components."""

from __future__ import annotations

import json
import threading
from pathlib import Path
from typing import Any, Callable, Optional

from antikythera_agent import host

_WASM_PATH = Path(__file__).parent / "antikythera.wasm"

#: WIT package/interface of the runner export in the composite.
_RUNNER_INTERFACE = "antikythera:agent-sdk/runner@1.0.0"


class WasmRuntimeError(Exception):
    """Error from WASM runtime."""


class WasmRuntime:
    """WASM runtime for executing antikythera components.

    Consumes the composite WASM (SDK + toolrunner) through the wasmtime
    component API (``wasmtime.component.Component`` + ``Linker.add_wasip2``),
    not the core-module ABI. The composite's single non-WASI import
    (``antikythera:agent-sdk/runtime-hooks@1.0.0``) is wired from
    ``antikythera_agent.host`` before instantiation. The runner functions are
    exported under the ``antikythera:agent-sdk/runner@1.0.0`` interface with
    WIT kebab-case names; ``call`` accepts both snake_case and kebab-case
    spellings.

    Example:
        >>> runtime = WasmRuntime()
        >>> result = runtime.call("init", '{"max_steps": 10}')
        >>> print(result)
    """

    def __init__(
        self,
        wasi_stdout: Optional[Callable[[str], None]] = None,
        wasi_stderr: Optional[Callable[[str], None]] = None,
        hooks_provider: Any = None,
    ) -> None:
        """Initialize WASM runtime.

        Args:
            wasi_stdout: Callback for WASI stdout output.
            wasi_stderr: Callback for WASI stderr output.
            hooks_provider: Hook implementation captured by this runtime instance.
                When omitted, captures the current host provider at construction.
        """
        self._call_lock = threading.RLock()
        self.hooks_provider = hooks_provider if hooks_provider is not None else host.get_provider()
        self._wasm_path = _WASM_PATH
        self._wasi_stdout = wasi_stdout or (lambda s: None)
        self._wasi_stderr = wasi_stderr or (lambda s: None)
        self._store: Any = None
        self._component: Any = None
        self._instance: Any = None
        self._linker: Any = None
        self._runner_export_index: Any = None
        self._initialized = False

    def _ensure_initialized(self) -> None:
        """Lazy-initialize the wasmtime component runtime.


        WasiConfig.stdout_custom / stderr_custom accept callables invoked with each chunk
        written by the guest, preserving the callback contract of wasi_stdout / wasi_stderr.
        """
        if self._initialized:
            return

        if not self._wasm_path.exists():
            raise WasmRuntimeError(
                f"WASM binary not found at {self._wasm_path}. "
                "Install with: pip install antikythera-agent"
            )

        try:
            import wasmtime
            import wasmtime.component
        except ImportError:
            raise WasmRuntimeError(
                "wasmtime is required for WASM execution. Install with: pip install wasmtime"
            )

        self._wasmtime = wasmtime

        engine = wasmtime.Engine()
        self._component = wasmtime.component.Component.from_file(engine, str(self._wasm_path))

        self._linker = wasmtime.component.Linker(engine)
        self._linker.add_wasip2()
        host.add_to_linker(self._linker, lambda: self.hooks_provider)

        wasi = wasmtime.WasiConfig()
        wasi.stdout_custom = self._wasi_stdout
        wasi.stderr_custom = self._wasi_stderr

        self._store = wasmtime.Store(engine)
        self._store.set_wasi(wasi)

        self._instance = self._linker.instantiate(self._store, self._component)
        self._runner_export_index = self._instance.get_export_index(self._store, _RUNNER_INTERFACE)
        if self._runner_export_index is None:
            raise WasmRuntimeError(
                f"Runner interface '{_RUNNER_INTERFACE}' not found in WASM "
                "component (is the binary the composite SDK + toolrunner?)"
            )

        self._initialized = True

    def call(self, func_name: str, args: str) -> str:
        """Serialize component calls and expose failures as WasmRuntimeError.

        The reentrant lock protects initialization and Wasmtime Store access
        for this instance. Calls to other runtime instances remain independent.
        """
        with self._call_lock:
            try:
                return self._call(func_name, args)
            except WasmRuntimeError:
                raise
            except Exception as exc:
                raise WasmRuntimeError(f"WASM call '{func_name}' failed: {exc}") from exc

    def _call(self, func_name: str, args: str) -> str:
        """Call an exported WASM runner function.

        The runner interface declares WIT kebab-case exports; ``func_name``
        is normalized from snake_case to kebab-case before lookup, so
        ``get_state`` and ``get-state`` name the same export.

        Result decoding (wasmtime component bindings):
        - ``result<string, string>`` arrives as a ``Variant`` with ``tag``
          ``'ok'``/``'err'``; ``'ok'`` yields its string payload, ``'err'``
          raises ``WasmRuntimeError`` with the payload.
        - ``result<u32, string>`` / ``result<bool, string>`` arrive as a
          bare ``int``/``bool`` on success and as the error ``str`` on
          failure; the error string raises ``WasmRuntimeError``, and numeric
          payloads are stringified so the return type stays ``str``.

        Functions with no parameters (``get-tools-prompt``) ignore ``args``.
        Functions with more than one parameter
        (``commit-llm-response``, ``process-llm-response-for-session``,
        ``process-tool-result-for-session``, ``append-llm-chunk``) take
        ``args`` as a JSON array spread positionally across the parameters
        (``null`` maps to WIT ``option`` values).

        Args:
            func_name: Name of the exported function.
            args: JSON string argument.

        Returns:
            JSON string result.

        Raises:
            WasmRuntimeError: If call fails.
        """
        self._ensure_initialized()

        try:
            normalized = func_name.replace("_", "-")
            func_idx = self._instance.get_export_index(
                self._store, normalized, self._runner_export_index
            )
            if func_idx is None:
                raise WasmRuntimeError(f"Export '{func_name}' not found in WASM")

            func = self._instance.get_func(self._store, func_idx)
            nparams = len(func.type(self._store).params)

            if nparams == 0:
                raw = func(self._store)
            elif nparams == 1:
                raw = func(self._store, args)
            else:
                try:
                    call_args = json.loads(args)
                except json.JSONDecodeError as e:
                    raise WasmRuntimeError(
                        f"WASM call '{func_name}' requires {nparams} arguments; "
                        f"pass a JSON array (invalid JSON: {e})"
                    ) from e
                if not isinstance(call_args, list) or len(call_args) != nparams:
                    raise WasmRuntimeError(
                        f"WASM call '{func_name}' requires {nparams} arguments "
                        f"as a JSON array; got {args!r}"
                    )
                raw = func(self._store, *call_args)

            return self._decode_result(func_name, raw)

        except WasmRuntimeError:
            raise
        except Exception as e:
            raise WasmRuntimeError(f"WASM call '{func_name}' failed: {e}") from e

    def _decode_result(self, func_name: str, raw: Any) -> str:
        """Decode a component-ABI return value into the string contract.


        Bare str is the err case of result<T, string> (the wasmtime component binding returns
        the error string directly when the ok payload is not a string).
        """
        if isinstance(raw, bool):
            return "True" if raw else "False"
        if isinstance(raw, int):
            return str(raw)
        if isinstance(raw, str):
            raise WasmRuntimeError(raw)
        if hasattr(raw, "tag") and hasattr(raw, "payload"):
            if raw.tag == "err":
                raise WasmRuntimeError(raw.payload)
            payload = raw.payload
            return payload if isinstance(payload, str) else str(payload)
        raise WasmRuntimeError(
            f"WASM call '{func_name}' returned unsupported type: {type(raw).__name__}"
        )

    def call_checked(self, func_name: str, args: str) -> dict[str, Any]:
        """Call WASM function and parse JSON result.

        Args:
            func_name: Name of the exported function.
            args: JSON string argument.

        Returns:
            Parsed result dictionary.

        Raises:
            WasmRuntimeError: If call or parsing fails.
        """
        result_json = self.call(func_name, args)
        try:
            return json.loads(result_json)
        except json.JSONDecodeError as e:
            raise WasmRuntimeError(f"Invalid JSON response from '{func_name}': {e}") from e
