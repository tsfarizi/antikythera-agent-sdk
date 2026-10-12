/**
 * node:test → bun:test compatibility shim.
 *
 * WHY: bun's resolver does not implement the `node:test` specifier, so
 * `bun test` fails on suites that import it. Bun honors tsconfig `paths`
 * at runtime, so this shim (mapped from `"node:test"` in ../tsconfig.json)
 * lets the same .mjs suite run under both runtimes. Node never reads
 * tsconfig paths and keeps its native node:test — this file is inert there.
 *
 * SEMANTIC DEVIATIONS (documented, accepted):
 *  1. Assertion style: suites use `node:assert/strict`, which bun implements
 *     natively; only the TEST RUNNER surface is shimmed here.
 *  2. `beforeEach`/`afterEach`: mapped to bun's same-named hooks when present;
 *     on builds lacking them they fall back to `beforeAll`/`afterAll`, which
 *     run ONCE per scope instead of per test — per-test isolation is weaker
 *     on that fallback path (bun >= 1.x always has the real hooks).
 *  3. `test(name, options, fn)`: the options object is forwarded verbatim;
 *     option keys unknown to bun (e.g. node's `skip` reason object shape) are
 *     ignored by bun rather than erroring.
 *  4. Node-only runner APIs (`test.skip`, `test.only` as property calls,
 *     `run()`) are NOT provided: neither target suite uses them.
 */

import * as bunTestModule from 'bun:test';

interface TestOptions {
  timeout?: number;
  skip?: boolean | string;
  todo?: boolean | string;
}

type TestBody = () => void | Promise<void>;

interface TestFn {
  (name: string, fn: TestBody): void;
  (name: string, options: TestOptions, fn: TestBody): void;
}

type HookFn = (fn: () => void | Promise<void>) => void;

const bunTest = bunTestModule as unknown as {
  test: TestFn;
  it: TestFn;
  describe: (name: string, fn: () => void) => void;
  beforeEach?: HookFn;
  beforeAll?: HookFn;
  afterEach?: HookFn;
  afterAll?: HookFn;
};

export const test: TestFn = ((name: string, optionsOrFn: TestOptions | TestBody, maybeFn?: TestBody) => {
  if (typeof optionsOrFn === 'function') {
    return bunTest.test(name, optionsOrFn);
  }
  return bunTest.test(name, optionsOrFn, maybeFn as TestBody);
}) as TestFn;

export const it: TestFn = test;

export const describe: (name: string, fn: () => void) => void = (name, fn) => bunTest.describe(name, fn);

/**
 * Real hooks when available; once-per-scope fallback otherwise (see deviation 2).
 */
export const beforeEach: HookFn =
  bunTest.beforeEach ?? ((fn) => (bunTest.beforeAll as HookFn)(fn));
export const afterEach: HookFn =
  bunTest.afterEach ?? ((fn) => (bunTest.afterAll as HookFn)(fn));
