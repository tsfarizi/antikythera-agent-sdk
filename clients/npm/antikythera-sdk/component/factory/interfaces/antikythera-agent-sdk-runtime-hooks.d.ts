/** @module Interface antikythera:agent-sdk/runtime-hooks@1.0.0 **/
export function prepareTurn(requestJson: string, sessionStateJson: string): string;
export function decideAction(sessionStateJson: string, llmResponseJson: string): string;
export function handleToolResult(sessionStateJson: string, toolResultJson: string): string;
