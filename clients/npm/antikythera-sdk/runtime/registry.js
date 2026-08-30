'use strict';

const { WIRE } = require('./types.js');

/**
 * Union registry (R5): the merged set of tool definitions from the client
 * side (local), the server side, and MCP servers. Each tool has exactly one
 * owner in {server, client, mcp}; a cross-side name collision is an explicit
 * error. The merged list is pushed to the runner in a single `register-tools`
 * call (register-tools replaces the whole registry).
 *
 * Parity contract (mirrors clients/python/antikythera_agent/server/registry.py and
 * antikythera-server-runtime/src/registry.rs):
 * - every definition is normalized to the golden 6-key shape;
 * - stored values are deep copies, so caller-side mutation cannot leak in and
 *   consumer-side mutation of toDefinitions() output cannot leak back in;
 * - toDefinitions() is sorted ascending by name (R5 determinism);
 * - a cross-owner collision throws the canonical Rust message with the
 *   EXISTING owner named first; same-owner re-registration replaces.
 */

/** Golden default input_schema when a definition provides none. */
const DEFAULT_INPUT_SCHEMA = Object.freeze({
  type: 'object',
  properties: {},
  required: [],
});

/**
 * Canonical R5 collision message (registry.rs:88-93); the existing owner is
 * always named first, the incoming owner second.
 */
function collisionMessage(name, existingOwner, owner) {
  return `tool registry: name collision for tool '${name}' (owners ${existingOwner}, ${owner})`;
}

/**
 * Validate a definition and reduce it to the golden 6-key shape
 * (`_normalize_definition` parity). Given values are kept as-is; only absent
 * fields fall back to defaults.
 */
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

/**
 * @param {object} options
 * @param {Array<{definition: object, handler: Function}>} [options.localEntries]
 * @param {Array<object>} [options.serverDefinitions]
 * @param {Array<object>} [options.mcpDefinitions]
 * @returns {{
 *   toDefinitions: () => Array<object>,
 *   ownerOf: (name: string) => string|undefined,
 *   has: (name: string) => boolean,
 *   size: () => number,
 * }}
 */
function createUnionRegistry(options = {}) {
  const byName = new Map();

  function add(definition, owner) {
    const normalized = structuredClone(normalizeDefinition(definition));
    const existing = byName.get(normalized.name);
    if (existing && existing.owner !== owner) {
      throw new Error(collisionMessage(normalized.name, existing.owner, owner));
    }
    byName.set(normalized.name, { owner, definition: normalized });
  }

  for (const entry of options.localEntries ?? []) {
    add(entry.definition, WIRE.OWNER_CLIENT);
  }
  for (const definition of options.serverDefinitions ?? []) {
    add(definition, WIRE.OWNER_SERVER);
  }
  for (const definition of options.mcpDefinitions ?? []) {
    add(definition, WIRE.OWNER_MCP);
  }

  return {
    toDefinitions() {
      return [...byName.values()]
        .map((entry) => structuredClone(entry.definition))
        .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    },
    ownerOf(name) {
      return byName.get(name)?.owner;
    },
    has(name) {
      return byName.has(name);
    },
    size() {
      return byName.size;
    },
    add(definition, owner) {
      return add(definition, owner);
    },
  };
}

module.exports = { createUnionRegistry };
