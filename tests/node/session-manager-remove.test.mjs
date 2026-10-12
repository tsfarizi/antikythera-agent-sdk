/**
 * Session-manager-remove.test.mjs — B3: SessionManager.remove() harus ber-semantik POP (hapus
 * entri + kembalikan nilainya), bukan PEEK. Implementasi saat ini
 * (clients/npm/antikythera-sdk/index.js:350-352) hanya menjalankan
 * `this.#sessions.get(sessionId) ?? null` TANPA delete — seluruh klausa penghapusan di bawah
 * teramati MERAH. Kontrak acuan: - clients/npm/antikythera-sdk/index.d.ts:306-310 — "Remove a
 * session. @returns Removed session or null" -> remove(sessionId): SessionInfo | null - Padanan
 * Python dict.pop: entri hilang, nilai lama dikembalikan sekali; pemanggilan berikutnya pada id
 * yang sama menghasilkan null. Klausa yang difalsifikasi (satu klaim per test): B3#1
 * remove(sessionId) mengembalikan SessionInfo milik entri DAN entri B3#2 count() turun tepat
 * satu setelah remove; sesi lain tak terpengaruh. B3#3 id tak dikenal — baik tak pernah ada
 * maupun sudah pernah dihapus — -> null tanpa melempar (idempotensi pop). B3#4 konsekuensi
 * penghapusan: getOrCreate sesudah remove membuat sesi BARU (messageCount kembali 0, bukan
 * melanjutkan entri lama). Mekanisme: unit murni tanpa I/O; setiap test membangun
 * SessionManager-nya sendiri (nol state bersama, lolos dalam urutan apa pun); deterministik
 * (waktu tidak diamati langsung — hanya keberadaan entri dan hitungan).
 */

'use strict';

import { test } from 'node:test';
import assert from 'node:assert/strict';

import sdk from '../../clients/npm/antikythera-sdk/index.js';

const { SessionManager } = sdk;

/**
 * B3#1 — remove mengembalikan SessionInfo entri dan MENGHAPUS entri tersebut.
 */

test('B3#1 remove(sessionId) mengembalikan SessionInfo-nya dan entri terhapus (get -> null)', () => {
  const mgr = new SessionManager();
  const created = mgr.getOrCreate('s-1', 'agent-a');
  assert.ok(created, 'pre-kondisi: getOrCreate harus membuat sesi');

  const removed = mgr.remove('s-1');

  /**
   * Klausa @returns: nilai balik adalah SessionInfo milik entri yang dihapus.
   */
  assert.ok(removed, 'remove harus mengembalikan SessionInfo entri yang dihapus');
  assert.deepEqual(removed, {
    sessionId: 's-1',
    agentId: 'agent-a',
    createdAt: created.createdAt,
    lastActivity: created.lastActivity,
    messageCount: 0,
  });

  /**
   * Klausa penghapusan: entri benar-benar hilang dari registry.
   */
  assert.equal(mgr.get('s-1'), null, 'get(sessionId) sesudah remove harus null');
});

/**
 * B3#2 — count() turun tepat satu; sesi lain tidak ikut terhapus.
 */

test('B3#2 count() turun satu setelah remove dan sesi lain tetap utuh', () => {
  const mgr = new SessionManager();
  mgr.getOrCreate('s-1', 'agent-a');
  mgr.getOrCreate('s-2', 'agent-a');
  assert.equal(mgr.count(), 2, 'pre-kondisi: dua sesi terdaftar');

  mgr.remove('s-1');

  assert.equal(mgr.count(), 1, 'count harus turun satu setelah remove');
  const survivor = mgr.get('s-2');
  assert.ok(survivor, 'sesi lain tidak boleh terpengaruh');
  assert.equal(survivor.sessionId, 's-2');
  assert.equal(mgr.listByAgent('agent-a').length, 1);
});

/**
 * B3#3 — id tak dikenal (tak pernah ada / sudah dihapus) -> null, tanpa throw.
 */

test('B3#3 remove pada id tak dikenal mengembalikan null tanpa melempar', () => {
  const mgr = new SessionManager();

  /**
   * Id yang tidak pernah terdaftar.
   */
  assert.doesNotThrow(() => mgr.remove('ghost'));
  assert.equal(mgr.remove('ghost'), null);

  /**
   * Id yang SUDAH pernah di-remove: pop kedua wajib null (idempotensi). ARM INI YANG MERAH pada
   * implementasi sekarang: karena entri tidak dihapus, pop kedua masih mengembalikan objek lama.
   */
  mgr.getOrCreate('s-1', 'agent-a');
  mgr.remove('s-1');
  assert.equal(mgr.remove('s-1'), null, 'pop kedua pada id yang sama harus null');
});

/**
 * B3#4 — konsekuensi penghapusan: getOrCreate sesudah remove = sesi BARU.
 */

test('B3#4 getOrCreate setelah remove membuat sesi baru (messageCount reset)', () => {
  const mgr = new SessionManager();
  const first = mgr.getOrCreate('s-1', 'agent-a');

  /**
   * Penanda pada objek hasil port publik: bila entri benar-benar dihapus, kreasi ulang wajib
   * menghasilkan entri segar dengan messageCount 0.
   */
  first.messageCount = 7;

  mgr.remove('s-1');
  const recreated = mgr.getOrCreate('s-1', 'agent-a');

  assert.ok(recreated, 'kreasi ulang setelah remove harus berhasil');
  assert.equal(
    recreated.messageCount,
    0,
    'sesi hasil kreasi ulang harus BARU (messageCount reset ke 0)',
  );
});
