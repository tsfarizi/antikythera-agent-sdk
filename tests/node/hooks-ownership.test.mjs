/**
 * Hooks-ownership.test.mjs — R3: kepemilikan hooks provider global berbasis token. Kontrak MIKRO
 * BARU (keputusan tunggal orkestrator, sumber acuan satu pintu) untuk
 * clients/npm/antikythera-sdk/runtime/control.js: - control.js mengekspor
 * acquireRuntimeHooksProvider(hooks) -> token opaque dan releaseRuntimeHooksProvider(token). -
 * Akuisisi memasang provider global pada globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__. -
 * Dua akuisisi koeksistan: release(token A) TIDAK boleh mencabut provider milik token B. -
 * Global dibersihkan HANYA saat jumlah pemilik kembali nol; urutan release bebas. - Release
 * token tak-dikenal / sudah-released = no-op aman (tanpa throw, tanpa mengganggu pemilik lain).
 * Implementasi saat ini BELUM mengekspor kedua fungsi (module.exports di
 * runtime/control.js:225-230 hanya createControlHandler, installRuntimeHooksProvider,
 * wrapHookFunction, invokeHook) -> seluruh test MERAH karena ekspor yang diharapkan absen.
 * Mekanisme falsifikasi: - Identitas "provider milik siapa" diverifikasi lewat MARKER PERILAKU
 * (hook decideAction mengembalikan string marker), bukan identitas objek — agar test tidak
 * kopling ke detail wrapping (wrapHookFunction). - Token diperlakukan OPAQUE: tidak pernah
 * diperiksa bentuk internalnya. - Higiene global sebelum/sesudah tiap test (konvensi
 * component-base.test.mjs:203-214) -> nol state bersama antar test.
 */

'use strict';

import { test, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';

import control from '../../clients/npm/antikythera-sdk/runtime/control.js';

const {
  acquireRuntimeHooksProvider,
  releaseRuntimeHooksProvider,
} = control;

const GLOBAL_SLOT = '__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__';

function clearRuntimeHooksProvider() {
  delete globalThis[GLOBAL_SLOT];
}

beforeEach(() => {
  clearRuntimeHooksProvider();
});

afterEach(() => {
  clearRuntimeHooksProvider();
});

/**
 * R3#1 — ekspor ada + akuisisi memasang provider global.
 */

test('R3#1 acquire/release diekspor dan akuisisi memasang provider global', () => {
  /**
   * Guard ekspor: pesan kegagalan harus menunjuk klausa ekspor, bukan TypeError.
   */
  assert.equal(
    typeof acquireRuntimeHooksProvider,
    'function',
    'control.js wajib mengekspor acquireRuntimeHooksProvider',
  );
  assert.equal(
    typeof releaseRuntimeHooksProvider,
    'function',
    'control.js wajib mengekspor releaseRuntimeHooksProvider',
  );

  const tokenA = acquireRuntimeHooksProvider({
    decideAction: () => 'marker-A',
  });
  assert.ok(tokenA !== undefined && tokenA !== null, 'akuisisi wajib mengembalikan token');

  const provider = globalThis[GLOBAL_SLOT];
  assert.ok(provider, `akuisisi wajib memasang globalThis.${GLOBAL_SLOT}`);
  assert.equal(typeof provider.decideAction, 'function', 'provider global mengekspos hook decideAction');
  assert.equal(provider.decideAction('', ''), 'marker-A', 'provider global terhubung ke hooks yang diakuisisi');
});

/**
 * R3#2 — dua akuisisi koeksistan: release(A) tidak mencabut provider B.
 */

test('R3#2 dua akuisisi koeksistan: release(token A) tidak menghapus provider milik token B', () => {
  const tokenA = acquireRuntimeHooksProvider({ decideAction: () => 'marker-A' });
  const tokenB = acquireRuntimeHooksProvider({ decideAction: () => 'marker-B' });

  /**
   * Akuisisi terakhir aktif secara global.
   */
  assert.equal(globalThis[GLOBAL_SLOT].decideAction('', ''), 'marker-B');

  /**
   * Release A tidak boleh mencabut provider milik B.
   */
  releaseRuntimeHooksProvider(tokenA);
  assert.ok(globalThis[GLOBAL_SLOT], 'provider global masih hidup selama B belum release');
  assert.equal(
    globalThis[GLOBAL_SLOT].decideAction('', ''),
    'marker-B',
    'provider yang tersisa harus milik token B',
  );
});

/**
 * R3#3 — urutan release bebas + cleanup tepat saat nol pemilik.
 */

test('R3#3 urutan release bebas dan global dibersihkan hanya saat semua token released', () => {
  const tokenA = acquireRuntimeHooksProvider({ decideAction: () => 'marker-A' });
  const tokenB = acquireRuntimeHooksProvider({ decideAction: () => 'marker-B' });

  /**
   * Release dalam urutan TERBALIK (B lebih dulu): provider A selamat.
   */
  releaseRuntimeHooksProvider(tokenB);
  assert.ok(globalThis[GLOBAL_SLOT], 'release B tidak boleh mencabut provider A');
  assert.equal(globalThis[GLOBAL_SLOT].decideAction('', ''), 'marker-A');

  /**
   * Transisi ke nol pemilik -> global bersih.
   */
  releaseRuntimeHooksProvider(tokenA);
  assert.equal(
    globalThis[GLOBAL_SLOT],
    undefined,
    `global ${GLOBAL_SLOT} wajib dibersihkan saat nol pemilik`,
  );
});

/**
 * R3#4 — siklus tunggal reusable + release tak-dikenal = no-op aman.
 */

test('R3#4 release token tak-dikenal no-op aman; siklus tunggal dapat dipakai ulang', () => {
  /**
   * Release sebelum ada akuisisi apa pun: tidak boleh melempar.
   */
  assert.doesNotThrow(() => releaseRuntimeHooksProvider({ fake: true }));
  assert.doesNotThrow(() => releaseRuntimeHooksProvider('never-issued-token'));

  /**
   * Token palsu tidak merusak pemilik sah.
   */
  const tokenA = acquireRuntimeHooksProvider({ decideAction: () => 'marker-A' });
  assert.doesNotThrow(() => releaseRuntimeHooksProvider('never-issued-token'));
  assert.equal(globalThis[GLOBAL_SLOT].decideAction('', ''), 'marker-A');

  /**
   * Double-release token yang sama: release kedua = no-op aman.
   */
  releaseRuntimeHooksProvider(tokenA);
  assert.doesNotThrow(() => releaseRuntimeHooksProvider(tokenA));
  assert.equal(globalThis[GLOBAL_SLOT], undefined);

  /**
   * Siklus dapat dipakai ulang setelah cleanup.
   */
  const tokenC = acquireRuntimeHooksProvider({ decideAction: () => 'marker-C' });
  assert.equal(globalThis[GLOBAL_SLOT].decideAction('', ''), 'marker-C');
  releaseRuntimeHooksProvider(tokenC);
  assert.equal(globalThis[GLOBAL_SLOT], undefined);
});
