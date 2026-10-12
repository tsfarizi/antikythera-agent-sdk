/** Verifies semantic filenames, loader consistency, and refusal of ambiguous jco bundles. */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const component = path.join(root, 'clients/npm/antikythera-sdk/component');
const script = path.join(root, 'src/scripts/post-transpile-rename.mjs');
const roles = ['runner', 'tool-registry', 'logic-hooks-passthrough'];
const namespaceNames = roles.flatMap((role) => [
  `antikythera-sdk.${role}.core.wasm`,
  `antikythera-sdk.${role}.wasi-preview1-adapter.core.wasm`,
]).sort();
const factoryNames = [...namespaceNames, ...roles.flatMap((role) => [
  `antikythera-sdk.${role}.import-trampolines.core.wasm`,
  `antikythera-sdk.${role}.import-fixups.core.wasm`,
]), 'antikythera-sdk.component-abi-adapters.core.wasm'].sort();

/** Captures exact file contents so refusal and idempotence checks detect partial mutations. */
function snapshot(directory) {
  return Object.fromEntries(fs.readdirSync(directory).sort()
    .filter((name) => name.endsWith('.wasm') || name === 'antikythera-sdk.js')
    .map((name) => [name, fs.readFileSync(path.join(directory, name))]));
}

/** Recreates opaque jco names in a different order while keeping the real generated loader. */
function opaqueFixture(t, relative = 'factory') {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'antikythera-core-names-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const original = snapshot(path.join(component, relative));
  const names = Object.keys(original).filter((name) => name.endsWith('.wasm'));
  const mapped = new Map(names.map((name, index) => [name, `antikythera-sdk.core${101 + names.length - index}.wasm`]));
  for (const [name, renamed] of mapped) fs.writeFileSync(path.join(directory, renamed), original[name]);
  const source = original['antikythera-sdk.js'].toString('utf8');
  const opaque = source.replace(/antikythera-sdk\.[a-z0-9.-]+\.wasm/g, (name) => mapped.get(name) ?? name);
  fs.writeFileSync(path.join(directory, 'antikythera-sdk.js'), opaque);
  return { directory, original, mapped };
}

function rename(directory) {
  return spawnSync(process.execPath, [script, directory], { encoding: 'utf8' });
}

for (const [relative, expected] of [['', namespaceNames], ['factory', factoryNames]]) {
  test(`published ${relative || 'namespace'} bundle uses descriptive names and matches Python`, () => {
    const directory = path.join(component, relative);
    const actual = snapshot(directory);
    assert.deepEqual(Object.keys(actual).filter((name) => name.endsWith('.wasm')), expected);
    const loader = actual['antikythera-sdk.js'].toString('utf8');
    const references = [...loader.matchAll(/antikythera-sdk\.[a-z0-9.-]+\.wasm/g)].map((match) => match[0]);
    assert.deepEqual([...new Set(references)].sort(), expected);
    assert.doesNotMatch(loader, /antikythera-sdk\.core\d*\.wasm/);
    for (const name of expected) assert.ok(WebAssembly.validate(actual[name]), name);
    const python = path.join(root, 'clients/python/antikythera_agent/component', relative);
    assert.deepEqual(snapshot(python), actual);
  });

  test(`${relative || 'namespace'} roles survive jco renumbering without changing binary bytes`, (t) => {
    const { directory, original } = opaqueFixture(t, relative);
    const result = rename(directory);
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(snapshot(directory), original);
    const second = rename(directory);
    assert.equal(second.status, 0, second.stderr);
    assert.match(second.stdout, /0 renamed/);
    assert.deepEqual(snapshot(directory), original);
  });
}

test('unknown module roles fail before changing binaries or loader', (t) => {
  const { directory } = opaqueFixture(t);
  fs.writeFileSync(path.join(directory, 'antikythera-sdk.core999.wasm'), Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]));
  const before = snapshot(directory);
  const result = rename(directory);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /unknown core-module role/);
  assert.deepEqual(snapshot(directory), before);
});

test('duplicate application modules fail before filename collisions can overwrite binaries', (t) => {
  const { directory, original } = opaqueFixture(t);
  fs.writeFileSync(path.join(directory, 'antikythera-sdk.core999.wasm'), original['antikythera-sdk.runner.core.wasm']);
  const before = snapshot(directory);
  const result = rename(directory);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /rename collision/);
  assert.deepEqual(snapshot(directory), before);
});

test('ambiguous import-fixup ownership fails without assigning an arbitrary component', (t) => {
  const { directory, original } = opaqueFixture(t);
  fs.writeFileSync(path.join(directory, 'antikythera-sdk.core999.wasm'), original['antikythera-sdk.runner.import-trampolines.core.wasm']);
  const before = snapshot(directory);
  const result = rename(directory);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /ambiguous import-fixup owner/);
  assert.deepEqual(snapshot(directory), before);
});

test('a missing loader module is detected before any rename', (t) => {
  const { directory, mapped } = opaqueFixture(t);
  const loader = path.join(directory, 'antikythera-sdk.js');
  const [oldName] = mapped.values();
  fs.writeFileSync(loader, fs.readFileSync(loader, 'utf8').replace(oldName, 'antikythera-sdk.core999.wasm'));
  const before = snapshot(directory);
  const result = rename(directory);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /loader references unclassified module/);
  assert.deepEqual(snapshot(directory), before);
});

test('an unreferenced core module is rejected before modifying the bundle', (t) => {
  const { directory, mapped } = opaqueFixture(t);
  const loader = path.join(directory, 'antikythera-sdk.js');
  const [oldName] = mapped.values();
  fs.writeFileSync(loader, fs.readFileSync(loader, 'utf8').replace(oldName, 'unrelated.wasm'));
  const before = snapshot(directory);
  const result = rename(directory);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /has no loader reference/);
  assert.deepEqual(snapshot(directory), before);
});
