"""Wire-compatible LLM providers: stub, Ollama, and OpenAI-compatible HTTP.

Streaming providers emit incremental UTF-8 content and return the complete
seven-field wire response only after a completion marker. HTTP/network and
malformed provider responses raise LlmError. The HTTP bridge selects streaming
using ?stream=true; metadata_json is never used as a stream flag.
"""

from __future__ import annotations

import json
import urllib.request
from abc import ABC, abstractmethod
from typing import Any, Callable, Dict, Optional

from . import wire

#: Provider name used when no explicit provider is given.
DEFAULT_PROVIDER = "stub"

#: Static body returned by the registry-default stub instance.
_DEFAULT_STUB_RESPONSE = '{"content": "stub response", "finish_reason": "stop"}'

#: HTTP transport signature: (url, body, timeout) -> parsed JSON object.
HttpTransport = Callable[[str, Dict[str, Any], float], Dict[str, Any]]


class LlmError(Exception):
    """Domain error for LLM/provider failures (never a raw HTTP exception)."""


class LlmProvider(ABC):
    """Base contract: `call(request)` -> golden `llm_call_response` shape."""

    def call(self, request: Dict[str, Any]) -> Dict[str, Any]:
        if not isinstance(request, dict):
            raise ValueError("llm provider request must be an object")
        return self._call(request)

    def call_stream(
        self, request: Dict[str, Any], on_token: Callable[[str], None]
    ) -> Dict[str, Any]:
        response = self.call(request)
        on_token(response["content"])
        return response

    @abstractmethod
    def _call(self, request: Dict[str, Any]) -> Dict[str, Any]:
        """Provider-specific request handling."""


class StubProvider(LlmProvider):
    """Return a configured static response for tests/parity."""

    def __init__(self, response_json: str = _DEFAULT_STUB_RESPONSE):
        try:
            body = json.loads(response_json)
        except json.JSONDecodeError as exc:
            raise ValueError(f"stub response_json is not valid JSON: {exc}") from exc
        if not isinstance(body, dict):
            raise ValueError("stub response_json must be a JSON object")
        self.response_json = response_json

    def _call(self, request: Dict[str, Any]) -> Dict[str, Any]:
        """Mirror `StubLlmProvider::call` (parity U61): `content` is the whole configured string
        verbatim — never the value of a `content` field in the configured JSON;
        model/session_id come from the request.

        Final normalization through the golden parser guarantees the seven-key invariant (no
        extra fields) while preserving the content.
        """
        parsed = {
            "content": self.response_json,
            "model": request.get("model"),
            "session_id": request.get("session_id"),
            "message_json": None,
            "tokens_used": 4,
            "finish_reason": "stop",
            "raw_response_json": None,
        }
        return wire.parse_llm_response(parsed)


def _urlopen_json_transport(url: str, body: Dict[str, Any], timeout: float) -> Dict[str, Any]:
    """Default transport: stdlib `urllib.request` POST returning parsed JSON."""
    payload = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=payload,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


class OllamaProvider(LlmProvider):
    """Minimal HTTP proxy to Ollama's `/api/chat` (zero-dependency base)."""

    def __init__(
        self,
        base_url: str = "http://127.0.0.1:11434",
        model: Optional[str] = None,
        timeout: float = 60.0,
        transport: Optional[HttpTransport] = None,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.model = model
        self.timeout = timeout
        self._transport = transport or _urlopen_json_transport

    def _call(self, request: Dict[str, Any]) -> Dict[str, Any]:
        """Every failure at the HTTP boundary is a domain error; raw URLError/HTTPError must
        never reach the caller.
        """
        url = self.base_url + "/api/chat"
        body = self._build_ollama_body(request)
        try:
            raw_response = self._transport(url, body, self.timeout)
        except Exception as exc:
            raise LlmError(f"ollama request to {url} failed: {exc}") from exc
        return self._parse_ollama_response(raw_response, request)

    def call_stream(
        self, request: Dict[str, Any], on_token: Callable[[str], None]
    ) -> Dict[str, Any]:
        if self._transport is not _urlopen_json_transport:
            return super().call_stream(request, on_token)
        body = self._build_ollama_body(request)
        body["stream"] = True
        req = urllib.request.Request(
            self.base_url + "/api/chat",
            data=json.dumps(body).encode(),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        full = ""
        try:
            with urllib.request.urlopen(req, timeout=self.timeout) as response:
                for line in response:
                    if len(line) > 8 * 1024 * 1024:
                        raise LlmError("stream record exceeds 8 MiB")
                    if not line.strip():
                        continue
                    value = json.loads(line.decode("utf-8"))
                    if value.get("error"):
                        raise LlmError(str(value["error"]))
                    delta = (value.get("message") or {}).get("content", "")
                    full += delta
                    if delta:
                        on_token(delta)
                    if value.get("done") is True:
                        value["message"] = {"role": "assistant", "content": full}
                        return self._parse_ollama_response(value, request)
            raise LlmError("stream ended without completion marker")
        except LlmError:
            raise
        except Exception as exc:
            raise LlmError(f"ollama stream failed: {exc}") from exc

    def _build_ollama_body(self, request: Dict[str, Any]) -> Dict[str, Any]:
        raw_messages = request.get("messages_json") or "[]"
        try:
            messages = json.loads(raw_messages)
        except (TypeError, json.JSONDecodeError) as exc:
            raise ValueError(f"llm-request messages_json is not valid JSON: {exc}") from exc
        if not isinstance(messages, list):
            raise ValueError("llm-request messages_json must decode to a JSON array")
        model = request.get("model") or self.model
        if not model:
            raise LlmError("ollama provider requires a model (request.model or provider model)")
        body = {"model": model, "messages": messages, "stream": False}
        if request.get("force_json") is True:
            body["format"] = "json"
        options = {}
        temperature = request.get("temperature")
        if temperature is not None:
            options["temperature"] = temperature
        max_tokens = request.get("max_tokens")
        if max_tokens is not None:
            options["num_predict"] = max_tokens
        if options:
            body["options"] = options
        return body

    def _parse_ollama_response(self, raw: Any, request: Dict[str, Any]) -> Dict[str, Any]:
        """Final normalization through the golden parser guarantees the seven-key invariant even
        if a future field mapping drifts.
        """
        if not isinstance(raw, dict):
            raise LlmError(f"ollama response is not a JSON object: {type(raw).__name__}")
        message = raw.get("message")
        content = ""
        message_json = None
        if isinstance(message, dict):
            message_content = message.get("content")
            content = message_content if isinstance(message_content, str) else ""
            message_json = json.dumps(message)
        eval_count = raw.get("eval_count") or 0
        prompt_eval_count = raw.get("prompt_eval_count") or 0
        tokens_used = None
        if eval_count or prompt_eval_count:
            tokens_used = prompt_eval_count + eval_count
        finish_reason = raw.get("done_reason")
        if finish_reason is None and raw.get("done") is True:
            finish_reason = "stop"
        parsed = {
            "content": content,
            "model": raw.get("model"),
            "session_id": request.get("session_id"),
            "message_json": message_json,
            "tokens_used": tokens_used,
            "finish_reason": finish_reason,
            "raw_response_json": json.dumps(raw),
        }
        return wire.parse_llm_response(parsed)


class OpenAiProvider(LlmProvider):
    """OpenAI-compatible chat completions using the same wire response as Rust."""

    def __init__(self, endpoint: str, api_key: str, model: str, timeout: float = 60.0):
        self.endpoint, self.api_key, self.model, self.timeout = (
            endpoint.rstrip("/"),
            api_key,
            model,
            timeout,
        )

    def _request(self, request: Dict[str, Any], stream: bool) -> Any:
        body = {
            "model": request.get("model") or self.model,
            "messages": json.loads(request.get("messages_json") or "[]"),
            "stream": stream,
        }
        if request.get("force_json"):
            body["response_format"] = {"type": "json_object"}
        for name in ("temperature", "max_tokens"):
            if request.get(name) is not None:
                body[name] = request[name]
        return urllib.request.Request(
            self.endpoint + "/chat/completions",
            data=json.dumps(body).encode(),
            headers={"Content-Type": "application/json", "Authorization": "Bearer " + self.api_key},
            method="POST",
        )

    def _response(
        self, request: Dict[str, Any], content: str, raw: Any = None, finish: str = "stop"
    ) -> Dict[str, Any]:
        return wire.parse_llm_response(
            {
                "content": content,
                "model": request.get("model") or self.model,
                "session_id": request.get("session_id"),
                "message_json": None,
                "tokens_used": (raw or {}).get("usage", {}).get("total_tokens"),
                "finish_reason": finish,
                "raw_response_json": json.dumps(raw) if raw else None,
            }
        )

    def _call(self, request: Dict[str, Any]) -> Dict[str, Any]:
        try:
            with urllib.request.urlopen(
                self._request(request, False), timeout=self.timeout
            ) as response:
                value = json.load(response)
            choice = value["choices"][0]
            return self._response(
                request,
                choice["message"].get("content") or "",
                value,
                choice.get("finish_reason") or "stop",
            )
        except Exception as exc:
            raise LlmError(f"openai call failed: {exc}") from exc

    def call_stream(
        self, request: Dict[str, Any], on_token: Callable[[str], None]
    ) -> Dict[str, Any]:
        full, finish = "", "stop"
        try:
            with urllib.request.urlopen(
                self._request(request, True), timeout=self.timeout
            ) as response:
                for line in response:
                    if len(line) > 8 * 1024 * 1024:
                        raise LlmError("stream record exceeds 8 MiB")
                    if not line.startswith(b"data:"):
                        continue
                    data = line[5:].strip()
                    if data == b"[DONE]":
                        return self._response(request, full, finish=finish)
                    value = json.loads(data.decode("utf-8"))
                    if value.get("error"):
                        raise LlmError(str(value["error"]))
                    if not value.get("choices"):
                        continue
                    choice = value["choices"][0]
                    delta = choice.get("delta", {}).get("content") or ""
                    full += delta
                    if delta:
                        on_token(delta)
                    finish = choice.get("finish_reason") or finish
            raise LlmError("stream ended without completion marker")
        except LlmError:
            raise
        except Exception as exc:
            raise LlmError(f"openai stream failed: {exc}") from exc


provider_registry: Dict[str, LlmProvider] = {
    "stub": StubProvider(),
    "ollama": OllamaProvider(),
}


def resolve_provider(name: Optional[str] = None) -> LlmProvider:
    """Resolve a registered provider; `None`/omitted -> default `stub`."""
    key = DEFAULT_PROVIDER if name is None else name
    if key not in provider_registry:
        raise KeyError(f"unknown LLM provider: {key!r}; known: {sorted(provider_registry)}")
    return provider_registry[key]
