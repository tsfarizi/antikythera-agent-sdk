/**
 * Host-side implementation of the `antikythera:agent-sdk/runtime-hooks@1.0.0` import, wired by
 * jco via the `-M` transpile mapping. The transpiled antikythera-sdk.js imports these three
 * named exports and calls them synchronously with decoded JSON strings; the returned string is
 * lifted as the Ok payload of the WIT `result<string, string>` and any thrown value is lifted as
 * the Err payload (a thrown string therefore surfaces to the guest as `Err(string)` — see
 * BUILD.md gate-error rule). Provider contract: set
 * `globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__` to an object exposing any subset of
 * `prepareTurn`, `decideAction`, `handleToolResult`. Each function has the signature `(a:
 * string, b: string) => string` with the argument order fixed by the WIT interface; it returns a
 * JSON decision string (`{"passthrough": true}` or an override object) and signals denial by
 * throwing a plain string (`throw "permission: ..."`). Default behavior is passthrough for all
 * three points when no provider is configured: the SDK keeps its own default decision.
 * Configuring a provider is opt-in — absence of a provider is never treated as a failure.
 */

function provider() {
  return globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__;
}

export function prepareTurn(requestJson, sessionStateJson) {
  const hook = provider()?.prepareTurn;
  if (typeof hook !== 'function') {
    return '{"passthrough": true}';
  }
  return hook(requestJson, sessionStateJson);
}

export function decideAction(sessionStateJson, llmResponseJson) {
  const hook = provider()?.decideAction;
  if (typeof hook !== 'function') {
    return '{"passthrough": true}';
  }
  return hook(sessionStateJson, llmResponseJson);
}

export function handleToolResult(sessionStateJson, toolResultJson) {
  const hook = provider()?.handleToolResult;
  if (typeof hook !== 'function') {
    return '{"passthrough": true}';
  }
  return hook(sessionStateJson, toolResultJson);
}
