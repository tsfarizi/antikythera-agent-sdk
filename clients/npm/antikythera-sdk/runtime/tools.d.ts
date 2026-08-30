export const DEFAULT_INPUT_SCHEMA: Readonly<{
  type: 'object';
  properties: Record<string, never>;
  required: readonly never[];
}>;

export const TOOL_SYMBOL: symbol;

export type ToolOwner = 'server' | 'client' | 'mcp';

export interface ToolDefinition {
  name: string;
  title?: string | null;
  description: string;
  parameters?: unknown[];
  input_schema?: Record<string, unknown> | null;
  output_schema?: Record<string, unknown> | null;
}

export type ToolHandler = (...args: any[]) => any;

export interface ToolEntry {
  definition: ToolDefinition;
  handler: ToolHandler;
  owner: ToolOwner;
}

export type ToolRef = ((...args: any[]) => any) & {
  readonly [key: symbol]: ToolEntry;
};

export function defineTool(
  definition: ToolDefinition,
  handler: ToolHandler,
  owner?: ToolOwner,
): ToolRef;

export function isTool(value: unknown): boolean;

export function extractToolEntry(value: unknown): ToolEntry | null;

export function registerTool(
  registry: any,
  gate: any,
  value: unknown,
  allow?: boolean,
): ToolEntry;
