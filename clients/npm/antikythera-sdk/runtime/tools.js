'use strict';

const DEFAULT_INPUT_SCHEMA = Object.freeze({
  type: 'object',
  properties: {},
  required: [],
});

const TOOL_SYMBOL = Symbol.for('antikythera.tool');

function normalizeDefinition(definition) {
  if (!definition || typeof definition !== 'object') {
    throw new Error('registry: tool definition requires an object');
  }
  if (typeof definition.name !== 'string' || !definition.name) {
    throw new Error('registry: tool definition requires a name');
  }
  if (typeof definition.description !== 'string' || !definition.description) {
    throw new Error('registry: tool definition requires a description');
  }
  return {
    name: definition.name,
    title: definition.title ?? null,
    description: definition.description,
    parameters: definition.parameters ?? [],
    input_schema: definition.input_schema ?? DEFAULT_INPUT_SCHEMA,
    output_schema: definition.output_schema ?? null,
  };
}

function defineTool(definition, handler, owner = 'server') {
  if (owner === undefined || owner === null) owner = 'server';
  const normalized = structuredClone(normalizeDefinition(definition));
  if (typeof handler !== 'function') {
    throw new Error(`tool '${normalized.name}': handler is required`);
  }
  if (typeof owner !== 'string' || !owner) {
    throw new Error(`registry: tool owner must be a non-empty string (got '${String(owner)}')`);
  }
  if (owner !== 'server' && owner !== 'client' && owner !== 'mcp') {
    throw new Error(`registry: tool owner must be one of server, client, mcp (got '${owner}')`);
  }
  function toolWrapper(...args) {
    return handler(...args);
  }
  try {
    Object.defineProperty(toolWrapper, 'name', { value: `tool:${normalized.name}`, configurable: true });
  } catch (_) {}
  toolWrapper[TOOL_SYMBOL] = {
    definition: normalized,
    handler,
    owner,
  };
  return toolWrapper;
}

function isTool(value) {
  if (value == null) return false;
  const t = typeof value;
  if (t !== 'object' && t !== 'function') return false;
  const entry = value[TOOL_SYMBOL];
  return !!entry && typeof entry === 'object' && entry !== null && typeof entry.handler === 'function' && typeof entry.definition === 'object' && entry.definition !== null && typeof entry.owner === 'string';
}

function extractToolEntry(value) {
  if (!isTool(value)) return null;
  const raw = value[TOOL_SYMBOL];
  return {
    definition: structuredClone(raw.definition),
    handler: raw.handler,
    owner: raw.owner,
  };
}

function registerTool(registry, gate, value, allow = false) {
  if (typeof allow !== 'boolean') {
    throw new TypeError("registerTool: 'allow' must be a bool");
  }
  const entry = extractToolEntry(value);
  if (!entry) {
    throw new Error('registry: not a tool');
  }
  if (registry != null) {
    if (typeof registry.add === 'function') {
      registry.add(entry.definition, entry.owner);
    } else if (typeof registry.register_server === 'function' || typeof registry.register_client === 'function' || typeof registry.register_mcp === 'function') {
      if (entry.owner === 'server' && typeof registry.register_server === 'function') {
        registry.register_server(entry.definition, entry.handler);
      } else if (entry.owner === 'client' && typeof registry.register_client === 'function') {
        registry.register_client(entry.definition);
      } else if (entry.owner === 'mcp' && typeof registry.register_mcp === 'function') {
        registry.register_mcp(entry.definition);
      } else if (typeof registry._register === 'function') {
        registry._register(entry.owner, entry.definition, entry.handler);
      }
    } else if (registry._tools instanceof Map) {
      const normalized = structuredClone(entry.definition);
      const existing = registry._tools.get(normalized.name);
      if (existing && existing.owner !== entry.owner) {
        throw new Error(`tool registry: name collision for tool '${normalized.name}' (owners ${existing.owner}, ${entry.owner})`);
      }
      registry._tools.set(normalized.name, { owner: entry.owner, definition: normalized, handler: entry.handler });
    }
  }
  if (allow && gate != null) {
    if (typeof gate.allowTool === 'function') {
      gate.allowTool(entry.owner, entry.definition.name);
    } else if (typeof gate.allow === 'function') {
      gate.allow(entry.definition.name);
    } else if (gate.allow instanceof Set) {
      gate.allow.add(entry.definition.name);
    } else if (Array.isArray(gate.allow)) {
      if (!gate.allow.includes(entry.definition.name)) gate.allow.push(entry.definition.name);
    }
  }
  return entry;
}

module.exports = { defineTool, isTool, extractToolEntry, registerTool, DEFAULT_INPUT_SCHEMA, TOOL_SYMBOL };
