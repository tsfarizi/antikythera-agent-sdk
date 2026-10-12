/**
 * Orchestrator-node.test.mjs — UN3 fase RED: Orchestrator Node + getVersion. Kontrak acuan: -
 * debug/U1-design-notes.md §E3.1 — class Orchestrator di atas
 * createAgentRuntime({core:'client'}) ; serverUrl WAJIB ; session-per-agent-profile ; getBudget
 * faktual. versi package.json (sumber kebenaran S7). -
 * clients/npm/antikythera-sdk/runtime/runner-core.js:389-477 — runTurn -> {sessionId,
 * action:'final', content, iterations} ; resetSession -> bool. Amanemen orkestrator (port S2
 * untuk test): Orchestrator menerima opsi `runtimeFactory(config) -> runtime-like` sebagai seam
 * injeksi; default = createAgentRuntime({core:'client', ...}). runtime-like minimal: connect(),
 * runTurn(prompt) -> {sessionId, action:'final', content, iterations}, resetSession() -> bool,
 * close(). Mock HANYA di seam kontrak publik (runtimeFactory); tidak ada penetrasi struktur
 * internal Orchestrator. Stub resetSession SELALU true sehingga Status fase: RED — modul belum
 * ada; SEMUA test di file ini wajib GAGAL karena `Orchestrator`/`getVersion` belum diekspor dari
 * index.js.
 */

'use strict';

import path from 'node:path';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import assert from 'node:assert/strict';

import sdk from '../../clients/npm/antikythera-sdk/index.js';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const PKG = JSON.parse(fs.readFileSync(path.resolve(HERE, '..', '..', 'clients', 'npm', 'antikythera-sdk', 'package.json'), 'utf8'));

const SERVER_URL = 'http://orchestrator.test';

/**
 * Guards — pesan RED menunjuk klausa yang dilanggar, bukan TypeError samar.
 */

function requireOrchestratorClass() {
  assert.equal(
    typeof sdk.Orchestrator,
    'function',
    'KLAUSA b (E3.2): class Orchestrator harus diekspor dari index.js',
  );
  return sdk.Orchestrator;
}

/**
 * Test double di seam publik runtimeFactory — runtime-like minimal (amanamen).
 */

/**
 * @param {string} name - nama runtime stub (jadi basis sessionId default)
 * @param {{sessionId?: string, iterations?: number, throwOnReset?: string,
 *          onRunTurn?: (prompt: string) => Promise<object>}} [opts]
 */
function makeStubFactory(name, opts = {}) {
  const states = []; /** Satu state per runtime instance yang dibuat factory. */
  const configs = []; /** Config yang diteruskan orchestrator ke factory. */
  const factory = async (config) => {
    configs.push(config);
    const state = {
      name,
      runTurnPrompts: [],
      resetCalls: 0,
      connectCalls: 0,
      closeCalls: 0,
    };
    states.push(state);
    return {
      core: 'client',
      connect: async () => {
        state.connectCalls += 1;
      },
      runTurn: async (prompt) => {
        state.runTurnPrompts.push(prompt);
        if (opts.onRunTurn) return opts.onRunTurn(prompt);
        return {
          sessionId: opts.sessionId ?? `${name}-sess`,
          action: 'final',
          content: `out(${prompt})`,
          iterations: opts.iterations ?? 3,
        };
      },
      resetSession: async () => {
        if (opts.throwOnReset) throw new Error(opts.throwOnReset);
        state.resetCalls += 1;
        return true; /** SELALU true — lihat header. */
      },
      close: () => {
        state.closeCalls += 1;
      },
    };
  };
  factory.states = states;
  factory.configs = configs;
  return factory;
}

/** Total panggilan runTurn lintas semua runtime yang dibuat factory. */
function totalRunTurnCalls(factory) {
  return factory.states.reduce((n, s) => n + s.runTurnPrompts.length, 0);
}


test('UN3#a1 getVersion diekspor dari index.js sebagai fungsi', () => {
  assert.equal(
    typeof sdk.getVersion,
    'function',
    'KLAUSA a (E3.3): getVersion harus diekspor dari index.js',
  );
});

test('UN3#a2 getVersion() mengembalikan string === versi package.json', () => {
  assert.equal(typeof sdk.getVersion, 'function', 'KLAUSA a (E3.3): getVersion harus diekspor');
  const version = sdk.getVersion();
  assert.equal(typeof version, 'string', 'getVersion() harus mengembalikan string');
  assert.equal(
    version,
    PKG.version,
    `KLAUSA a (E3.3/S7): getVersion() harus === package.json version (${PKG.version})`,
  );
});

/**
 * KLAUSA b — Orchestrator diekspor (E3.2: class nyata, bukan fantom).
 */

test('UN3#b class Orchestrator diekspor dari index.js', () => {
  assert.equal(
    typeof sdk.Orchestrator,
    'function',
    'KLAUSA b (E3.2): class Orchestrator harus diekspor dari index.js sebagai konstruktor',
  );
});

/**
 * KLAUSA c — pre-kondisi serverUrl WAJIB (mirror createClientCoreRuntime:178).
 */

test('UN3#c new Orchestrator({}) tanpa serverUrl -> throw Error eksplisit', () => {
  const Orchestrator = requireOrchestratorClass();
  assert.throws(
    () => new Orchestrator({}),
    (err) => err instanceof Error && /serverUrl/i.test(err.message),
    'KLAUSA c (E3.1): serverUrl WAJIB — tanpa serverUrl harus throw Error eksplisit yang menyebut serverUrl',
  );
});

/**
 * KLAUSA d — registerAgent/listAgents: profil tersimpan sebagai salinan.
 */

test('UN3#d registerAgent + listAgents -> profil tersimpan sebagai salinan (mutasi hasil tidak bocor)', () => {
  const Orchestrator = requireOrchestratorClass();
  const orch = new Orchestrator({ serverUrl: SERVER_URL });
  orch.registerAgent({ id: 'a1', name: 'Alpha', role: 'worker', systemPrompt: 'sys', maxSteps: 4 });

  const listed = orch.listAgents();
  assert.deepEqual(
    listed,
    [{ id: 'a1', name: 'Alpha', role: 'worker', systemPrompt: 'sys', maxSteps: 4 }],
    'KLAUSA d: listAgents mengembalikan profil lengkap sesuai yang diregistrasi',
  );

  /**
   * Mutasi HASIL tidak boleh bocor ke registry internal.
   */
  listed[0].name = 'HACKED';
  listed.pop();
  const again = orch.listAgents();
  assert.equal(again.length, 1, 'KLAUSA d: mutasi array hasil tidak boleh memengaruhi registry');
  assert.equal(again[0].name, 'Alpha', 'KLAUSA d: mutasi objek hasil tidak boleh bocor ke profil tersimpan');
  assert.equal(again[0].maxSteps, 4, 'KLAUSA d: field profil tersimpan tetap utuh');
});

/**
 * KLAUSA e — dispatch via runtimeFactory stub: runTurn 1x + TaskResult shape.
 */

test('UN3#e dispatch via runtimeFactory stub -> runTurn sekali dengan prompt benar + TaskResult shape lengkap', async () => {
  const Orchestrator = requireOrchestratorClass();
  const factory = makeStubFactory('alpha-runtime', { sessionId: 'sess-alpha', iterations: 4 });
  const orch = new Orchestrator({ serverUrl: SERVER_URL, runtimeFactory: factory });
  orch.registerAgent({ id: 'agent-alpha', name: 'Alpha', role: 'worker', systemPrompt: 'sys-alpha', maxSteps: 5 });

  const result = await orch.dispatch('tugas');

  assert.equal(factory.states.length, 1, 'KLAUSA e: satu profil -> satu runtime (session-per-profile)');
  assert.equal(
    factory.states[0].runTurnPrompts.length,
    1,
    'KLAUSA e: runTurn harus dipanggil TEPAT sekali per dispatch',
  );
  assert.equal(
    factory.states[0].runTurnPrompts[0],
    'tugas',
    'KLAUSA e: runTurn dipanggil dengan prompt apa adanya',
  );

  /**
   * TaskResult shape (E3.1 / index.d.ts TaskResult).
   */
  assert.equal(typeof result.taskId, 'string', 'KLAUSA e: taskId harus string');
  assert.ok(result.taskId.length > 0, 'KLAUSA e: taskId non-empty');
  assert.equal(result.agentId, 'agent-alpha', 'KLAUSA e: agentId === profil.id');
  assert.equal(result.output, 'out(tugas)', 'KLAUSA e: output === content stub runTurn');
  assert.equal(result.success, true, 'KLAUSA e: dispatch sukses -> success=true');
  assert.equal(result.stepsUsed, 4, 'KLAUSA e: stepsUsed === iterations stub (faktual runTurn)');
  assert.equal(result.sessionId, 'sess-alpha', 'KLAUSA e: sessionId dari runtime');
  assert.equal(typeof result.durationMs, 'number', 'KLAUSA e: durationMs harus number');
  assert.ok(result.durationMs >= 0, 'KLAUSA e: durationMs >= 0');
});

/**
 * KLAUSA f — budget faktual (update atomik setelah TaskResult).
 */

test('UN3#f1 budget faktual: 2 dispatch sukses -> dispatchedTasks=2, consumedSteps>0, exhausted=false', async () => {
  const Orchestrator = requireOrchestratorClass();
  const factory = makeStubFactory('budget-runtime', { iterations: 3 });
  const orch = new Orchestrator({
    serverUrl: SERVER_URL,
    maxTotalTasks: 3,
    runtimeFactory: factory,
  });
  orch.registerAgent({ id: 'b1', name: 'Budget', role: 'worker' });

  await orch.dispatch('t1');
  await orch.dispatch('t2');

  const budget = orch.getBudget();
  assert.equal(budget.dispatchedTasks, 2, 'KLAUSA f: dispatchedTasks terhitung faktual per TaskResult');
  assert.ok(budget.consumedSteps > 0, 'KLAUSA f: consumedSteps > 0 (akumulasi stepsUsed faktual)');
  assert.equal(
    budget.isTaskBudgetExhausted,
    false,
    'KLAUSA f: budget belum habis (2 dari maxTotalTasks=3)',
  );
});

test('UN3#f2 budget habis: dispatch berikutnya ditolak success=false tanpa memanggil runTurn lagi', async () => {
  const Orchestrator = requireOrchestratorClass();
  const factory = makeStubFactory('budget-cap', { iterations: 2 });
  const orch = new Orchestrator({
    serverUrl: SERVER_URL,
    maxTotalTasks: 2,
    runtimeFactory: factory,
  });
  orch.registerAgent({ id: 'b2', name: 'Cap', role: 'worker' });

  await orch.dispatch('t1');
  await orch.dispatch('t2');
  assert.equal(
    orch.getBudget().isTaskBudgetExhausted,
    true,
    'KLAUSA f: maxTotalTasks tercapai -> isTaskBudgetExhausted=true',
  );

  const rejected = await orch.dispatch('t3-over-budget');
  assert.equal(rejected.success, false, 'KLAUSA f: dispatch di atas budget ditolak (success=false)');
  assert.ok(rejected.error, 'KLAUSA f: penolakan menyertakan pesan error');
  assert.equal(
    totalRunTurnCalls(factory),
    2,
    'KLAUSA f: runTurn TIDAK boleh dipanggil lagi setelah budget habis',
  );
});

/**
 * KLAUSA g — cancel: true -> resetSession runtime pemilik; idempoten; error raise.
 */

test('UN3#g1 cancel(sessionId) pertama -> true dan resetSession dipanggil di runtime pemilik session saja', async () => {
  const Orchestrator = requireOrchestratorClass();
  const factoryA = makeStubFactory('rt-a', { sessionId: 'sess-A' });
  const factoryB = makeStubFactory('rt-b', { sessionId: 'sess-B' });
  /**
   * Factory delegasi: pembuatan runtime pertama utk profil pertama, pembuatan kedua utk profil
   * kedua (session-per-profile).
   */
  let creations = 0;
  const factory = async (config) => {
    creations += 1;
    return creations === 1 ? factoryA(config) : factoryB(config);
  };

  const orch = new Orchestrator({ serverUrl: SERVER_URL, runtimeFactory: factory });
  orch.registerAgent({ id: 'ag-a', name: 'A', role: 'worker' });
  orch.registerAgent({ id: 'ag-b', name: 'B', role: 'worker' });

  const dispatched = await orch.dispatch('t');
  assert.equal(dispatched.sessionId, 'sess-A', 'prasyarat: dispatch pertama memakai agent pertama');
  await orch.dispatch('t-b', { agentId: 'ag-b' });
  assert.equal(factoryB.states.length, 1, 'explicit selection initializes the second profile');

  const cancelled = await orch.cancel('sess-A');
  assert.equal(cancelled, true, 'KLAUSA g: cancel sessionId aktif -> true');
  assert.equal(factoryA.states[0].resetCalls, 1, 'KLAUSA g: resetSession runtime PEMILIK sess-A dipanggil');
  assert.equal(factoryB.states[0].resetCalls, 0, 'KLAUSA g: runtime lain TIDAK ikut di-reset');
});

test('UN3#g2 cancel ulang sessionId yang sama -> false (idempoten)', async () => {
  const Orchestrator = requireOrchestratorClass();
  const factory = makeStubFactory('rt-idem', { sessionId: 'sess-idem' });
  const orch = new Orchestrator({ serverUrl: SERVER_URL, runtimeFactory: factory });
  orch.registerAgent({ id: 'ag-i', name: 'I', role: 'worker' });
  await orch.dispatch('t');

  const first = await orch.cancel('sess-idem');
  assert.equal(first, true, 'prasyarat: cancel pertama true');
  const again = await orch.cancel('sess-idem');
  assert.equal(
    again,
    false,
    'KLAUSA g: cancel ulang id sama harus false — stub resetSession selalu true, jadi hanya idempotensi orchestrator yang bisa menghasilkan false',
  );
  assert.equal(
    factory.states[0].resetCalls,
    1,
    'KLAUSA g: idempoten — resetSession runtime tidak dipanggil ulang untuk id yang sama',
  );
});

test('UN3#g3 resetSession yang melempar error program -> cancel raise (bukan ditelan jadi false)', async () => {
  const Orchestrator = requireOrchestratorClass();
  const factory = makeStubFactory('rt-err', { sessionId: 'sess-err', throwOnReset: 'reset program failure' });
  const orch = new Orchestrator({ serverUrl: SERVER_URL, runtimeFactory: factory });
  orch.registerAgent({ id: 'ag-e', name: 'E', role: 'worker' });
  await orch.dispatch('t');

  await assert.rejects(
    () => orch.cancel('sess-err'),
    /reset program failure/,
    'KLAUSA g: error program dari resetSession harus di-propagate, bukan dikonversi jadi false',
  );
});

/**
 * KLAUSA h — pipeline chaining + kegagalan tengah.
 */

test('UN3#h1 pipeline sukses -> prompt task kedua mengandung output task pertama + PipelineResult shape', async () => {
  const Orchestrator = requireOrchestratorClass();
  const factory = makeStubFactory('pipe-runtime', { sessionId: 'sess-pipe', iterations: 2 });
  const orch = new Orchestrator({ serverUrl: SERVER_URL, runtimeFactory: factory });
  orch.registerAgent({ id: 'p1', name: 'Pipe', role: 'worker' });

  const result = await orch.pipeline(['a', 'b']);

  assert.equal(factory.states[0].runTurnPrompts[0], 'a', 'KLAUSA h: task pertama dieksekusi dengan prompt apa adanya');
  assert.ok(
    typeof factory.states[0].runTurnPrompts[1] === 'string' &&
      factory.states[0].runTurnPrompts[1].includes('out(a)'),
    `KLAUSA h: prompt task kedua HARUS mengandung output task pertama 'out(a)', got: ${JSON.stringify(factory.states[0].runTurnPrompts[1])}`,
  );
  assert.equal(result.success, true, 'KLAUSA h: pipeline sukses -> success=true');
  assert.equal(result.results.length, 2, 'KLAUSA h: results memuat hasil tiap task');
  assert.equal(
    result.finalOutput,
    result.results[result.results.length - 1].output,
    'KLAUSA h: finalOutput === output task terakhir',
  );
  assert.equal(typeof result.totalSteps, 'number', 'KLAUSA h: totalSteps number');
  assert.ok(result.totalSteps > 0, 'KLAUSA h: totalSteps > 0 (akumulasi langkah faktual)');
});

test('UN3#h2 pipeline kegagalan tengah -> success=false dan results berhenti di titik gagal', async () => {
  const Orchestrator = requireOrchestratorClass();
  let calls = 0;
  const factory = makeStubFactory('pipe-fail', {
    onRunTurn: async (prompt) => {
      calls += 1;
      if (calls === 2) throw new Error('mid-pipeline boom');
      return { sessionId: 'sess-pf', action: 'final', content: `out(${prompt})`, iterations: 1 };
    },
  });
  const orch = new Orchestrator({ serverUrl: SERVER_URL, runtimeFactory: factory });
  orch.registerAgent({ id: 'pf', name: 'PF', role: 'worker' });

  const result = await orch.pipeline(['a', 'b', 'c']);

  assert.equal(result.success, false, 'KLAUSA h: kegagalan tengah -> success=false (PipelineResult, bukan reject)');
  assert.equal(result.results.length, 1, 'KLAUSA h: results berhenti — hanya task sebelum kegagalan');
  assert.equal(calls, 2, 'KLAUSA h: eksekusi berhenti setelah kegagalan (task ketiga tidak dieksekusi)');
  assert.ok(result.error, 'KLAUSA h: PipelineResult.error terisi');
  assert.match(String(result.error), /mid-pipeline boom/, 'KLAUSA h: error memuat penyebab kegagalan');
});

/**
 * KLAUSA i — dispatchMany concurrent: bukti overlap + hasil terurut input.
 */

test('UN3#i dispatchMany concurrent (maxConcurrentTasks=2) -> kedua task overlap sebelum ada yang selesai + hasil terurut input', async () => {
  const Orchestrator = requireOrchestratorClass();

  const events = [];
  let resolveGate;
  const gate = new Promise((resolve) => {
    resolveGate = resolve;
  });
  let timedOut = false;
  /**
   * Deadline eksplisit 5s: jika eksekusi ternyata sequential, gate tak pernah terbuka oleh task
   * kedua -> fallback ini mencegah hang dan menandai gagal.
   */
  const deadline = setTimeout(() => {
    timedOut = true;
    resolveGate();
  }, 5000);

  const factory = makeStubFactory('conc-runtime', {
    onRunTurn: async (prompt) => {
      events.push(`enter:${prompt}`);
      if (events.filter((e) => e.startsWith('enter:')).length === 2) resolveGate();
      await gate;
      events.push(`exit:${prompt}`);
      return { sessionId: `sess-${prompt}`, action: 'final', content: `done(${prompt})`, iterations: 2 };
    },
  });

  try {
    const orch = new Orchestrator({
      serverUrl: SERVER_URL,
      executionMode: 'concurrent',
      maxConcurrentTasks: 2,
      runtimeFactory: factory,
    });
    orch.registerAgent({ id: 'ci', name: 'Conc', role: 'worker' });

    const results = await orch.dispatchMany(['alpha', 'beta']);

    assert.equal(
      timedOut,
      false,
      'KLAUSA i: gate harus terbuka oleh kedua task sebelum deadline 5s — timeout berarti eksekusi TIDAK konkuren',
    );
    const enters = events.filter((e) => e.startsWith('enter:'));
    assert.equal(enters.length, 2, 'KLAUSA i: kedua task harus mulai dieksekusi');
    const firstExitIdx = events.findIndex((e) => e.startsWith('exit:'));
    const secondEnterIdx = events.indexOf(enters[1]);
    assert.ok(
      secondEnterIdx < firstExitIdx,
      `KLAUSA i: bukti konkurensi — task kedua (${enters[1]}) harus masuk SEBELUM task pertama selesai; timeline: ${events.join(' | ')}`,
    );
    assert.deepEqual(
      results.map((r) => r.output),
      ['done(alpha)', 'done(beta)'],
      'KLAUSA i: hasil dispatchMany harus terurut sesuai urutan input (bukan urutan penyelesaian)',
    );
  } finally {
    clearTimeout(deadline);
  }
});

/**
 * KLAUSA j — dispatchMany sequential: satu runtime sama, urutan terjaga.
 */

test('UN3#j dispatchMany sequential -> memakai satu runtime yang sama untuk satu profil, urutan panggilan terjaga', async () => {
  const Orchestrator = requireOrchestratorClass();
  const factory = makeStubFactory('seq-runtime', {});
  const orch = new Orchestrator({
    serverUrl: SERVER_URL,
    executionMode: 'sequential',
    runtimeFactory: factory,
  });
  orch.registerAgent({ id: 'sj', name: 'Seq', role: 'worker' });

  const results = await orch.dispatchMany(['first', 'second', 'third']);

  assert.deepEqual(
    factory.states[0].runTurnPrompts,
    ['first', 'second', 'third'],
    'KLAUSA j: urutan panggilan runTurn mengikuti urutan input',
  );
  assert.equal(
    factory.states.length,
    1,
    'KLAUSA j: sequential memakai SATU runtime yang sama untuk satu profil (session-per-profile; factory tidak dipanggil ulang per task)',
  );
  assert.deepEqual(
    results.map((r) => r.output),
    ['out(first)', 'out(second)', 'out(third)'],
    'KLAUSA j: hasil berurutan sesuai input',
  );
});
