/**
 * Each call instantiates the jco Component Model factory with private imports. Compiled core
 * modules are immutable and can be shared between instances.
 */
import { instantiate } from './factory/antikythera-sdk.js';
import * as environment from './wasi-stubs/environment.js';
import * as exit from './wasi-stubs/exit.js';
import * as stderr from './wasi-stubs/stderr.js';
import * as stdin from './wasi-stubs/stdin.js';
import * as stdout from './wasi-stubs/stdout.js';
import * as monotonicClock from './wasi-stubs/monotonic-clock.js';
import * as wallClock from './wasi-stubs/wall-clock.js';
import * as preopens from './wasi-stubs/preopens.js';
import * as types from './wasi-stubs/types.js';
import * as error from './wasi-stubs/error.js';
import * as streams from './wasi-stubs/streams.js';
import * as random from './wasi-stubs/random.js';

const modules = new Map();
/**
 * Compile a core module once per resolved URL and share its immutable code.
 * Failed compilations leave the cache so a subsequent call can retry loading.
 * WebAssembly memories and component state are allocated by instantiate().
 * @param {string} name - Core module path emitted by jco.
 * @returns {Promise<WebAssembly.Module>} Compiled module for factory instantiation.
 */
function getCoreModule(name) {
  const url = new URL(name, new URL('./factory/', import.meta.url));
  if (!modules.has(url.href)) {
    const pending = (async () => {
      const bytes = url.protocol === 'file:'
        ? await (await import('node:fs/promises')).readFile(url)
        : await fetch(url).then(async response => {
          if (!response.ok) throw new Error(`component module HTTP ${response.status}: ${url}`);
          return response.arrayBuffer();
        });
      return WebAssembly.compile(bytes);
    })();
    modules.set(url.href, pending);
    pending.catch(() => modules.delete(url.href));
  }
  return modules.get(url.href);
}

/**
 * Instantiate the jco component with private synchronous runtime hooks.
 * Missing hook methods return the WIT passthrough decision. Hook exceptions
 * are returned to the guest as error strings; asynchronous hooks are rejected.
 * Versioned WIT imports also receive the unversioned names used by the factory.
 * @param {object} [hooks] - prepareTurn, decideAction, and handleToolResult callbacks.
 * @returns {Promise<object>} Runner exports belonging to the new component instance.
 */
export async function createRunner(hooks = {}) {
  const imports = {
    'antikythera:agent-sdk/runtime-hooks@1.0.0': Object.fromEntries(
      ['prepareTurn', 'decideAction', 'handleToolResult'].map(name => [name, (a, b) => {
        if (typeof hooks?.[name] !== 'function') return '{"passthrough":true}';
        try {
          const result = hooks[name](a, b);
          if (result?.then) throw new Error('runtime hooks must be synchronous');
          return typeof result === 'string' ? result : JSON.stringify(result);
        } catch (err) { throw err instanceof Error ? err.message : String(err); }
      }]),
    ),
    'wasi:cli/environment@0.2.3': environment,
    'wasi:cli/exit@0.2.3': exit,
    'wasi:cli/stderr@0.2.3': stderr,
    'wasi:cli/stdin@0.2.3': stdin,
    'wasi:cli/stdout@0.2.3': stdout,
    'wasi:clocks/monotonic-clock@0.2.3': monotonicClock,
    'wasi:clocks/wall-clock@0.2.3': wallClock,
    'wasi:filesystem/preopens@0.2.3': preopens,
    'wasi:filesystem/types@0.2.3': types,
    'wasi:io/error@0.2.3': error,
    'wasi:io/streams@0.2.3': streams,
    'wasi:random/random@0.2.3': random,
  };
  for (const [name, value] of Object.entries(imports)) imports[name.split('@')[0]] = value;
  return (await instantiate(getCoreModule, imports)).runner;
}
