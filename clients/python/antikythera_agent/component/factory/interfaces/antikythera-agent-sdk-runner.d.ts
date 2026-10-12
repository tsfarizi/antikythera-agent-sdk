/** @module Interface antikythera:agent-sdk/runner@1.0.0 **/
export function init(configJson: string): string;
export function prepareUserTurn(requestJson: string): string;
export function commitLlmResponse(preparedTurnJson: string, llmResponseJson: string): string;
export function commitLlmStream(preparedTurnJson: string): string;
export function processLlmResponseForSession(sessionId: string, llmResponseJson: string): string;
export function processToolResultForSession(sessionId: string, toolResultJson: string): string;
export function appendLlmChunk(sessionId: string, chunk: string, correlationId: string | undefined): boolean;
export function drainEvents(sessionId: string): string;
export function getState(sessionId: string): string;
export function resetSession(sessionId: string): boolean;
export function sweepIdleSessions(nowUnixMs: bigint | undefined): number;
export function registerTools(toolsJson: string): number;
export function getToolsPrompt(): string;
export function setContextPolicy(policyJson: string): boolean;
export function getTelemetrySnapshot(sessionId: string): string;
export function getSloSnapshot(sessionId: string): string;
