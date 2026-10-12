"""MCP tool client: stdio and Streamable HTTP, bounded synchronous requests."""

from __future__ import annotations

import json
import os
import queue
import subprocess
import threading
import time
import urllib.request
from typing import Any


class McpClient:
    def __init__(self, config: dict[str, Any]):
        if not isinstance(config, dict):
            raise ValueError("MCP config must be an object")
        self.url = config.get("url") or config.get("endpoint")
        command = config.get("command")
        if bool(self.url) == bool(command):
            raise ValueError("MCP config requires exactly one of url or command")
        self.timeout = float(config.get("timeout", 30))
        if self.timeout <= 0:
            raise ValueError("MCP timeout must be positive")
        self.headers = dict(config.get("headers") or {})
        self.protocol = "2025-06-18"
        self.session = None
        self.process = None
        self._messages: queue.Queue[Any] = queue.Queue()
        self._lock = threading.Lock()
        self._seq = 0
        if command:
            argv = [command, *config.get("args", [])] if isinstance(command, str) else list(command)
            self.process = subprocess.Popen(
                argv,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                text=True,
                encoding="utf-8",
                bufsize=1,
                env={**os.environ, **(config.get("env") or {})},
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
            )
            process = self.process
            assert process.stdout is not None
            stdout = process.stdout

            def read() -> None:
                try:
                    for line in stdout:
                        self._messages.put(json.loads(line))
                except Exception as exc:
                    self._messages.put(exc)
                finally:
                    self._messages.put(EOFError("MCP stdio closed"))

            threading.Thread(target=read, daemon=True, name="antikythera-mcp-reader").start()
        try:
            result = self.request(
                "initialize",
                {
                    "protocolVersion": self.protocol,
                    "capabilities": {},
                    "clientInfo": {"name": "antikythera-agent", "version": "1.8.5"},
                },
            )
            self.protocol = result.get("protocolVersion", self.protocol)
            self.request("notifications/initialized", {}, notification=True)
        except Exception:
            self.close()
            raise

    def request(self, method: str, params: dict, *, notification: bool = False) -> Any:
        with self._lock:
            self._seq += 1
            message: dict[str, Any] = {"jsonrpc": "2.0", "method": method, "params": params}
            if not notification:
                message["id"] = self._seq
            deadline = time.monotonic() + self.timeout
            if self.process:
                assert self.process.stdin is not None
                self.process.stdin.write(json.dumps(message, ensure_ascii=False) + "\n")
                self.process.stdin.flush()
                if notification:
                    return None
                while True:
                    value = self._messages.get(timeout=max(0.001, deadline - time.monotonic()))
                    if isinstance(value, Exception):
                        raise value
                    if value.get("id") == message["id"]:
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError("MCP request deadline exceeded")
            else:
                headers = {
                    **self.headers,
                    "Content-Type": "application/json",
                    "Accept": "application/json, text/event-stream",
                    "MCP-Protocol-Version": self.protocol,
                }
                if self.session:
                    headers["Mcp-Session-Id"] = self.session
                assert isinstance(self.url, str)
                req = urllib.request.Request(
                    self.url, data=json.dumps(message).encode(), headers=headers, method="POST"
                )
                with urllib.request.urlopen(req, timeout=self.timeout) as response:
                    self.session = response.headers.get("Mcp-Session-Id") or self.session
                    if notification:
                        return None
                    if "text/event-stream" in response.headers.get("Content-Type", ""):
                        data = []
                        value = None
                        for line in response:
                            if time.monotonic() >= deadline:
                                raise TimeoutError("MCP request deadline exceeded")
                            line = line.decode("utf-8").rstrip("\r\n")
                            if line.startswith("data:"):
                                data.append(line[5:].lstrip(" "))
                            elif not line and data:
                                event = json.loads("\n".join(data))
                                data = []
                                if event.get("id") == message["id"]:
                                    value = event
                                    break
                        if value is None:
                            raise ValueError("MCP SSE ended without response")
                    else:
                        value = json.load(response)
            if value.get("id") != message["id"]:
                raise ValueError("MCP response id mismatch")
            if "error" in value:
                raise RuntimeError(f"MCP error: {value['error']}")
            if "result" not in value:
                raise ValueError("MCP response has no result")
            return value["result"]

    def tools(self) -> list[dict]:
        tools, cursor = [], None
        while True:
            result = self.request("tools/list", {"cursor": cursor} if cursor else {})
            tools.extend(result.get("tools", []))
            next_cursor = result.get("nextCursor")
            if not next_cursor:
                return tools
            if next_cursor == cursor:
                raise ValueError("MCP tool cursor did not advance")
            cursor = next_cursor

    def call_tool(self, name: str, arguments: Any) -> Any:
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        if result.get("isError"):
            raise RuntimeError(f"MCP tool failed: {result.get('content')}")
        return result.get("structuredContent", result.get("content", result))

    def close(self) -> None:
        if self.process and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=2)
        if self.process:
            if self.process.stdin is not None:
                self.process.stdin.close()
            if self.process.stdout is not None:
                self.process.stdout.close()
