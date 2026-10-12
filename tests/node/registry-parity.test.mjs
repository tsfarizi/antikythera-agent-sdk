/**
 * Registry-parity.test.mjs — R4 + K2: createUnionRegistry (Node) wajib ber-paritas penuh dengan
 * UnionRegistry Rust dan Python: R4 — collision lintas owner melempar Error dengan pesan PERSIS
 * kanonik Rust (implementasi JS saat ini memakai kalimat berbeda di runtime/registry.js:34-36 ->
 * MERAH pada string equality). K2 — definisi dinormalkan ke shape golden 6-kunci, disalin dalam
 * (deep-copy), hasil terurut ascending by name (implementasi JS saat ini pass-through referensi
 * tanpa normalisasi/urutan -> MERAH). Kontrak acuan (ground truth kanonik): -
 * antikythera-server-runtime/src/registry.rs:88-93 — format! string pesan collision; doc
 * komentar kanonik di registry.rs:82. Owner EXISTING selalu disebut lebih dulu, owner
 * penggantinya kedua. - clients/python/antikythera_agent/server/registry.py:108-111 — mirror
 * pesan yang sama; - clients/python/antikythera_agent/server/registry.py:38-67
 * (_normalize_definition) — shape golden 6 kunci; :28 DEFAULT_INPUT_SCHEMA; -
 * clients/python/antikythera_agent/server/registry.py:130-138 — definitions() terurut per nama,
 * salinan, "tidak ada kunci lain". Pesan kanonik yang DIPIN (sumber:
 * antikythera-server-runtime/src/registry.rs:88-93): tool registry: name collision for tool
 * '<name>' (owners <existing>, <new>) Catatan pemetaan owner (runtime/types.js:21-23):
 * localEntries -> 'client', serverDefinitions -> 'server', mcpDefinitions -> 'mcp'. API JS
 * memproses localEntries LEBIH DULU, sehingga duplikasi client-lalu-server menghasilkan "(owners
 * client, server)" — urutan existing-first dari template Rust tetap yang diverifikasi.
 * Mekanisme: unit murni tanpa I/O/WASM; shape ekspor WASM tidak difabrikasi; setiap test
 * membangun registry-nya sendiri (nol state bersama).
 */

'use strict';

import { test } from 'node:test';
import assert from 'node:assert/strict';

import registry from '../../clients/npm/antikythera-sdk/runtime/registry.js';

const { createUnionRegistry } = registry;

/**
 * PIN kanonik — JANGAN ubah tanpa mengubah sumbernya. Sumber:
 * antikythera-server-runtime/src/registry.rs:88-93 format!("tool registry: name collision for
 * tool '{}' (owners {}, {})", name, existing.owner.as_str(), owner.as_str()) Doc kanonik:
 * registry.rs:82; mirror Python: registry.py:109-110.
 */

function canonicalCollision(name, existingOwner, newOwner) {
  return `tool registry: name collision for tool '${name}' (owners ${existingOwner}, ${newOwner})`;
}

/**
 * Shape golden 6 kunci kanonik (registry.py:58-67) — urutan kunci dipin agar serialisasi
 * registry-sync identik lintas bahasa.
 */
const GOLDEN_KEYS = ['name', 'title', 'description', 'parameters', 'input_schema', 'output_schema'];

/**
 * Default input_schema saat tidak diberikan (registry.py:28).
 */
const DEFAULT_INPUT_SCHEMA = { type: 'object', properties: {}, required: [] };

/**
 * K2#1 — pre-kondisi: description harus string non-kosong (kelas input).
 */

test('K2#1 definition tanpa description string non-kosong -> throw Error', () => {
  const arms = [
    ['tanpa kunci description', { name: 't1' }],
    ['description string kosong', { name: 't2', description: '' }],
    ['description non-string', { name: 't3', description: 42 }],
  ];
  for (const [label, definition] of arms) {
    assert.throws(
      () => createUnionRegistry({ serverDefinitions: [definition] }),
      (err) => {
        assert.ok(err instanceof Error, `${label}: harus melempar Error`);
        /**
         * Mirror Python registry.py:55 ("... requires a description").
         */
        assert.match(err.message, /description/, `${label}: pesan harus menyebut description`);
        return true;
      },
      `K2#1 gagal pada arm: ${label}`,
    );
  }
});

/**
 * K2#2 — definisi minimal dinormalkan ke PERSIS 6 kunci golden + default.
 */

test('K2#2 definisi valid dinormalkan ke 6 kunci golden persis dengan nilai default', () => {
  const reg = createUnionRegistry({
    serverDefinitions: [
      /**
       * Kunci liar ("owner_extra") tidak boleh bocor ke hasil (registry.py:133).
       */
      { name: 'echo', description: 'd', owner_extra: 'junk' },
    ],
  });

  const defs = reg.toDefinitions();
  assert.equal(defs.length, 1);
  const out = defs[0];

  /**
   * Persis 6 kunci, urutan kanonik (registry.py:58-67).
   */
  assert.deepEqual(Object.keys(out), GOLDEN_KEYS);

  /**
   * Nilai default: title/output_schema null, parameters [], input_schema golden.
   */
  assert.deepEqual(out, {
    name: 'echo',
    title: null,
    description: 'd',
    parameters: [],
    input_schema: DEFAULT_INPUT_SCHEMA,
    output_schema: null,
  });
});

/**
 * K2#3 — nilai yang DIBERIKAN dipertahankan apa adanya (mirror `??`).
 */

test('K2#3 field yang diberikan dipertahankan: title/parameters/schema tidak ditimpa default', () => {
  const reg = createUnionRegistry({
    serverDefinitions: [
      {
        name: 'rich',
        title: 'Rich Tool',
        description: 'full',
        parameters: [{ name: 'p1', type: 'string' }],
        input_schema: { type: 'object', properties: { p1: { type: 'string' } }, required: ['p1'] },
        output_schema: { type: 'string' },
      },
    ],
  });

  const out = reg.toDefinitions()[0];
  assert.deepEqual(Object.keys(out), GOLDEN_KEYS);
  assert.deepEqual(out, {
    name: 'rich',
    title: 'Rich Tool',
    description: 'full',
    parameters: [{ name: 'p1', type: 'string' }],
    input_schema: { type: 'object', properties: { p1: { type: 'string' } }, required: ['p1'] },
    output_schema: { type: 'string' },
  });
});

/**
 * K2#4 — deep-copy: mutasi objek input SETELAH registrasi tidak bocor.
 */

test('K2#4 mutasi objek definition setelah registrasi tidak mengubah toDefinitions()', () => {
  const definition = {
    name: 'copycat',
    title: 'T',
    description: 'original',
    parameters: [{ name: 'p1' }],
    input_schema: { type: 'object', properties: { a: { type: 'string' } }, required: ['a'] },
    output_schema: null,
  };
  const reg = createUnionRegistry({ serverDefinitions: [definition] });

  /**
   * Mutasi PASCA-registrasi lewat referensi yang dipegang pemanggil.
   */
  definition.description = 'MUTATED';
  definition.title = 'MUTATED';
  definition.parameters.push({ name: 'p2' });
  definition.input_schema.properties.b = { type: 'number' };

  const out = reg.toDefinitions()[0];
  assert.equal(out.description, 'original', 'description tidak boleh ikut termutasi');
  assert.equal(out.title, 'T');
  assert.deepEqual(out.parameters, [{ name: 'p1' }]);
  assert.deepEqual(out.input_schema, {
    type: 'object',
    properties: { a: { type: 'string' } },
    required: ['a'],
  });
});

/**
 * K2#5 — toDefinitions() terurut ascending by name (determinisme R5).
 */

test('K2#5 toDefinitions() terurut ascending by name lintas owner', () => {
  const reg = createUnionRegistry({
    serverDefinitions: [{ name: 'zulu', description: 'z' }],
    localEntries: [{ definition: { name: 'alpha', description: 'a' }, handler: () => {} }],
    mcpDefinitions: [{ name: 'mike', description: 'm' }],
  });

  const names = reg.toDefinitions().map((d) => d.name);
  /**
   * Insertion order API = client -> server -> mcp = alpha,zulu,mike; kontrak menuntut ascending:
   * alpha,mike,zulu.
   */
  assert.deepEqual(names, ['alpha', 'mike', 'zulu']);
});

/**
 * R4#1 — collision lintas owner melempar Error dengan pesan PERSIS kanonik.
 */

test("R4#1 collision lintas owner melempar Error berpesan persis kanonik Rust ('client' existing)", () => {
  const dupClient = { name: 'dup_tool', description: 'milik client' };
  const dupServer = { name: 'dup_tool', description: 'milik server' };
  const dupMcp = { name: 'dup_tool', description: 'milik mcp' };

  /**
   * Arm 1: client terdaftar lebih dulu, server datang kedua -> existing='client', new='server'.
   */
  assert.throws(
    () => createUnionRegistry({
      localEntries: [{ definition: dupClient, handler: () => {} }],
      serverDefinitions: [dupServer],
    }),
    (err) => {
      assert.ok(err instanceof Error, 'collision harus melempar Error');
      assert.equal(err.message, canonicalCollision('dup_tool', 'client', 'server'));
      return true;
    },
  );

  /**
   * Arm 2: pemilik tetap 'client', pihak ketiga 'mcp' -> existing='client', new='mcp'.
   */
  assert.throws(
    () => createUnionRegistry({
      localEntries: [{ definition: dupClient, handler: () => {} }],
      mcpDefinitions: [dupMcp],
    }),
    (err) => err instanceof Error && err.message === canonicalCollision('dup_tool', 'client', 'mcp'),
  );
});

test("R4#1b collision arah sebaliknya: existing 'server' vs incoming 'mcp'", () => {
  assert.throws(
    () => createUnionRegistry({
      serverDefinitions: [{ name: 'srv_tool', description: 's' }],
      mcpDefinitions: [{ name: 'srv_tool', description: 'm' }],
    }),
    (err) => err instanceof Error && err.message === canonicalCollision('srv_tool', 'server', 'mcp'),
  );
});

/**
 * R4#2 — re-registrasi owner sama = replace, TIDAK throw (klausa penjaga regresi; implementasi
 * saat ini sudah benar untuk klausa ini).
 */

test('R4#2 re-registrasi owner sama mengganti entri lama tanpa throw', () => {
  const reg = createUnionRegistry({
    localEntries: [
      { definition: { name: 'echo', description: 'v1' }, handler: () => {} },
      { definition: { name: 'echo', description: 'v2' }, handler: () => {} },
    ],
  });

  assert.equal(reg.size(), 1, 're-registrasi owner sama tidak menambah entri');
  const out = reg.toDefinitions();
  assert.equal(out.length, 1);
  assert.equal(out[0].description, 'v2', 'definisi terakhir yang menang');
});
