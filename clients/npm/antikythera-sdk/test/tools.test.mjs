// tools.test.mjs — Parity JS sugar layer (mirror Python test_tools_sugar.py)
// CC-1..CC-4 JS parity: defineTool, DEFAULT deepCopy, collision existing-first,
// same-owner replace, register != allow, isTool, extractToolEntry deepCopy,
// normalizeLocalTools sugar unwrap, createUnionRegistry add + size.

'use strict';

import { test } from 'node:test';
import assert from 'node:assert/strict';

import tools from '../runtime/tools.js';
import registryMod from '../runtime/registry.js';
import policy from '../runtime/policy.js';
import types from '../runtime/types.js';

const { defineTool, isTool, extractToolEntry, registerTool, DEFAULT_INPUT_SCHEMA, TOOL_SYMBOL } = tools;
const { createUnionRegistry } = registryMod;
const { createPolicyGate } = policy;
const { normalizeLocalTools } = types;

// ---------------------------------------------------------------------------
// defineTool requires name/description
// ---------------------------------------------------------------------------

test('defineTool requires non-empty name and description', () => {
  assert.throws(() => defineTool({ name: '', description: 'd' }, () => {}), /name/);
  assert.throws(() => defineTool({ name: 'a', description: '' }, () => {}), /description/);
  assert.throws(() => defineTool({ description: 'd' }, () => {}), /name/);
  assert.throws(() => defineTool({ name: 'a' }, () => {}), /description/);
  assert.throws(() => defineTool(null, () => {}), /object/);
  assert.throws(() => defineTool({ name: 'a', description: 'd' }, null), /handler/);
  assert.throws(() => defineTool({ name: 'a', description: 'd' }, 'not-fn'), /handler/);
});

test('defineTool owner validates server|client|mcp', () => {
  for (const owner of ['server', 'client', 'mcp']) {
    const t = defineTool({ name: `t_${owner}`, description: 'd' }, () => 1, owner);
    assert.equal(isTool(t), true);
    assert.equal(extractToolEntry(t).owner, owner);
  }
  assert.throws(() => defineTool({ name: 'a', description: 'd' }, () => 1, 'invalid'), /owner/);
  assert.throws(() => defineTool({ name: 'a', description: 'd' }, () => 1, ''), /owner/);
});

// ---------------------------------------------------------------------------
// DEFAULT deepCopy
// ---------------------------------------------------------------------------

test('DEFAULT deepCopy: defineTool default input_schema cloned, mutation not leaking', () => {
  const t = defineTool({ name: 'def_js', description: 'd' }, () => 1);
  const entry = extractToolEntry(t);
  assert.deepEqual(entry.definition.input_schema, DEFAULT_INPUT_SCHEMA);
  // not same reference
  assert.notEqual(entry.definition.input_schema, DEFAULT_INPUT_SCHEMA);
  // mutate returned
  entry.definition.input_schema.properties.x = { type: 'string' };
  const entry2 = extractToolEntry(t);
  assert.deepEqual(entry2.definition.input_schema, DEFAULT_INPUT_SCHEMA);
  assert.equal('x' in entry2.definition.input_schema.properties, false);

  // mutate DEFAULT should not affect new tools (DEFAULT is frozen, but ensure clone)
  const t2 = defineTool({ name: 'def_js2', description: 'd2' }, () => 1);
  assert.deepEqual(extractToolEntry(t2).definition.input_schema, { type: 'object', properties: {}, required: [] });
});

// ---------------------------------------------------------------------------
// extractToolEntry deepCopy
// ---------------------------------------------------------------------------

test('extractToolEntry deepCopy: mutating returned definition does not leak', () => {
  const t = defineTool({ name: 'copy_js', description: 'd', input_schema: { type: 'object', properties: { a: { type: 'string' } }, required: ['a'] } }, () => 42);
  const e1 = extractToolEntry(t);
  e1.definition.input_schema.properties.a = { type: 'number' };
  e1.definition.description = 'MUTATED';
  const e2 = extractToolEntry(t);
  assert.equal(e2.definition.description, 'd');
  assert.deepEqual(e2.definition.input_schema.properties.a, { type: 'string' });
});

// ---------------------------------------------------------------------------
// collision existing-first
// ---------------------------------------------------------------------------

test('collision existing-first: server vs client same name -> owners server, client', () => {
  const reg = createUnionRegistry();
  reg.add({ name: 'dup', description: 'srv' }, 'server');
  assert.throws(() => reg.add({ name: 'dup', description: 'cli' }, 'client'), (err) => {
    assert.match(err.message, /collision/);
    assert.match(err.message, /dup/);
    assert.equal(err.message, `tool registry: name collision for tool 'dup' (owners server, client)`);
    return true;
  });
});

test('collision reverse: client first then server -> owners client, server', () => {
  const reg = createUnionRegistry();
  reg.add({ name: 'dup', description: 'cli' }, 'client');
  assert.throws(() => reg.add({ name: 'dup', description: 'srv' }, 'server'), (err) => {
    assert.equal(err.message, `tool registry: name collision for tool 'dup' (owners client, server)`);
    return true;
  });
});

test('collision server vs mcp', () => {
  const reg = createUnionRegistry();
  reg.add({ name: 'dup2', description: 'srv' }, 'server');
  assert.throws(() => reg.add({ name: 'dup2', description: 'mcp' }, 'mcp'), (err) => {
    assert.equal(err.message, `tool registry: name collision for tool 'dup2' (owners server, mcp)`);
    return true;
  });
});

// ---------------------------------------------------------------------------
// same-owner replace
// ---------------------------------------------------------------------------

test('same-owner replace: register same owner twice -> size 1, last wins', () => {
  const reg = createUnionRegistry();
  reg.add({ name: 'echo', description: 'old', title: 'Old' }, 'server');
  assert.equal(reg.size(), 1);
  assert.equal(reg.toDefinitions()[0].description, 'old');
  reg.add({ name: 'echo', description: 'new', title: 'New' }, 'server');
  assert.equal(reg.size(), 1);
  assert.equal(reg.toDefinitions()[0].description, 'new');
  assert.equal(reg.toDefinitions()[0].title, 'New');
});

test('same-owner replace via sugar tools', () => {
  const reg = createUnionRegistry();
  const gate = createPolicyGate({});
  const t1 = defineTool({ name: 'echo2', description: 'old' }, () => 'old', 'server');
  const t2 = defineTool({ name: 'echo2', description: 'new' }, () => 'new', 'server');
  registerTool(reg, gate, t1);
  assert.equal(reg.size(), 1);
  registerTool(reg, gate, t2);
  assert.equal(reg.size(), 1);
  assert.equal(reg.toDefinitions()[0].description, 'new');
});

// ---------------------------------------------------------------------------
// register != allow
// ---------------------------------------------------------------------------

test('register != allow strict: allow false not in gate, allow true in gate', () => {
  const reg = createUnionRegistry();
  const gate = createPolicyGate({ allow: [] });
  const t = defineTool({ name: 'allowme', description: 'd' }, () => 1, 'server');
  registerTool(reg, gate, t, false);
  assert.equal(reg.has('allowme'), true);
  assert.equal(gate.allows('allowme'), false);
  assert.throws(() => gate.check('allowme'), /permission:/);

  const reg2 = createUnionRegistry();
  const gate2 = createPolicyGate({ allow: [] });
  registerTool(reg2, gate2, t, true);
  assert.equal(gate2.allows('allowme'), true);
  gate2.check('allowme'); // should not throw
});

test('registerTool deepCopy via registry: mutation of input not leaking', () => {
  const reg = createUnionRegistry();
  const gate = createPolicyGate({});
  const t = defineTool({ name: 'deep_reg', description: 'd', input_schema: { type: 'object', properties: { a: { type: 'string' } }, required: [] } }, () => 1);
  registerTool(reg, gate, t);
  const defs = reg.toDefinitions();
  defs[0].input_schema.properties.a = { type: 'number' };
  const defs2 = reg.toDefinitions();
  assert.deepEqual(defs2[0].input_schema.properties.a, { type: 'string' });
});

// ---------------------------------------------------------------------------
// isTool
// ---------------------------------------------------------------------------

test('isTool false for plain dict, null, string, non-tool', () => {
  assert.equal(isTool({ name: 'a' }), false);
  assert.equal(isTool(null), false);
  assert.equal(isTool('string'), false);
  assert.equal(isTool(123), false);
  assert.equal(isTool([]), false);
  assert.equal(isTool({}), false);
  function plain() {}
  assert.equal(isTool(plain), false);
});

test('isTool true for defineTool, false for plain function', () => {
  const t = defineTool({ name: 'is_tool_fn', description: 'd' }, () => 1);
  assert.equal(isTool(t), true);
  assert.equal(isTool(() => 1), false);
  // decorated function has TOOL_SYMBOL
  assert.equal(typeof t[TOOL_SYMBOL], 'object');
});

// ---------------------------------------------------------------------------
// normalizeLocalTools sugar unwrap
// ---------------------------------------------------------------------------

test('normalizeLocalTools sugar unwrap', () => {
  const sugar = defineTool({ name: 'sugar', description: 'd' }, () => 42, 'server');
  const normalized = normalizeLocalTools([sugar]);
  assert.equal(normalized.length, 1);
  assert.equal(normalized[0].definition.name, 'sugar');
  assert.equal(typeof normalized[0].handler, 'function');
  // handler should still execute
  assert.equal(normalized[0].handler(), 42);

  // mixed sugar + plain entry
  const plainEntry = { definition: { name: 'plain', description: 'd' }, handler: () => 99 };
  const mixed = normalizeLocalTools([sugar, plainEntry]);
  assert.equal(mixed.length, 2);
  assert.deepEqual(mixed.map(e => e.definition.name), ['sugar', 'plain']);
});

test('normalizeLocalTools validates handler required', () => {
  assert.throws(() => normalizeLocalTools([{ definition: { name: 'a', description: 'd' }, handler: null }]), /handler/);
  assert.throws(() => normalizeLocalTools([{ definition: { name: '', description: 'd' }, handler: () => {} }]), /name/);
});

// ---------------------------------------------------------------------------
// createUnionRegistry add + size + sort determinism
// ---------------------------------------------------------------------------

test('createUnionRegistry add + size + sort determinism', () => {
  const reg = createUnionRegistry();
  assert.equal(reg.size(), 0);
  assert.deepEqual(reg.toDefinitions(), []);
  reg.add({ name: 'zebra', description: 'z' }, 'server');
  reg.add({ name: 'apple', description: 'a' }, 'server');
  assert.equal(reg.size(), 2);
  const names = reg.toDefinitions().map(d => d.name);
  assert.deepEqual(names, ['apple', 'zebra']);
});

test('createUnionRegistry toDefinitions deepCopy isolation', () => {
  const reg = createUnionRegistry();
  reg.add({ name: 'iso', description: 'd', title: 'T' }, 'client');
  const defs = reg.toDefinitions();
  defs[0].description = 'MUTATED';
  defs[0].title = 'MUTATED';
  const defs2 = reg.toDefinitions();
  assert.equal(defs2[0].description, 'd');
  assert.equal(defs2[0].title, 'T');
});

test('createUnionRegistry golden 6 keys exact', () => {
  const reg = createUnionRegistry();
  reg.add({ name: 'shape', description: 'd', title: 'T', parameters: [{ name: 'p' }], input_schema: { type: 'object', properties: {}, required: [] }, output_schema: { type: 'string' } }, 'server');
  const out = reg.toDefinitions()[0];
  assert.deepEqual(Object.keys(out).sort(), ['description', 'input_schema', 'name', 'output_schema', 'parameters', 'title'].sort());
  assert.equal(Object.keys(out).length, 6);
});
