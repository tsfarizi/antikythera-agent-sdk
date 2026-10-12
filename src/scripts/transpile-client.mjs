#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const component = path.join(root, 'clients/npm/antikythera-sdk/component');
const jco = path.join(root, 'clients/node_modules/@bytecodealliance/jco/dist/jco.js');
if (process.argv.includes('--clean')) {
  for (const relative of ['', 'factory']) {
    const dir = path.join(component, relative);
    if (!fs.existsSync(dir)) continue;
    for (const name of fs.readdirSync(dir)) {
      if (/^antikythera-sdk(?:\..*)?\.(?:js|d\.ts|wasm)$/.test(name)) {
        fs.unlinkSync(path.join(dir, name));
      }
    }
  }
  console.log('Generated jco entries removed; adapters and WASI stubs preserved');
  process.exit(0);
}

function run(script, args) {
  const result = spawnSync(process.execPath, [script, ...args], { cwd: root, stdio: 'inherit' });
  if (result.status !== 0) process.exit(result.status ?? 1);
}
const mappings = {
  'antikythera:agent-sdk/runtime-hooks@1.0.0': './runtime-hooks.js',
  'wasi:cli/environment@0.2.3': './wasi-stubs/environment.js',
  'wasi:cli/exit@0.2.3': './wasi-stubs/exit.js',
  'wasi:cli/stderr@0.2.3': './wasi-stubs/stderr.js',
  'wasi:cli/stdin@0.2.3': './wasi-stubs/stdin.js',
  'wasi:cli/stdout@0.2.3': './wasi-stubs/stdout.js',
  'wasi:clocks/monotonic-clock@0.2.3': './wasi-stubs/monotonic-clock.js',
  'wasi:clocks/wall-clock@0.2.3': './wasi-stubs/wall-clock.js',
  'wasi:filesystem/preopens@0.2.3': './wasi-stubs/preopens.js',
  'wasi:filesystem/types@0.2.3': './wasi-stubs/types.js',
  'wasi:io/error@0.2.3': './wasi-stubs/error.js',
  'wasi:io/streams@0.2.3': './wasi-stubs/streams.js',
  'wasi:random/random@0.2.3': './wasi-stubs/random.js',
};
/**
 * Removes only generated wasm files; preserve hand-authored adapters and stubs.
 */
for (const name of fs.readdirSync(component)) {
  if (/^antikythera-sdk\..*\.wasm$/.test(name)) fs.unlinkSync(path.join(component, name));
}
run(jco, ['transpile', 'dist/antikythera-sdk.wasm', '--out-dir', component,
  ...Object.entries(mappings).flatMap(([key, value]) => ['-M', `${key}=${value}`])]);
run(path.join(root, 'src/scripts/post-transpile-rename.mjs'), []);
const factoryDir = path.join(component, 'factory');
if (fs.existsSync(factoryDir)) {
  for (const name of fs.readdirSync(factoryDir)) {
    if (/^antikythera-sdk\..*\.wasm$/.test(name)) fs.unlinkSync(path.join(factoryDir, name));
  }
}
run(jco, ['transpile', 'dist/antikythera-sdk.wasm', '--instantiation', 'async',
  '--no-wasi-shim', '--name', 'antikythera-sdk', '--out-dir', path.join(component, 'factory')]);
run(path.join(root, 'src/scripts/post-transpile-rename.mjs'), [factoryDir]);
/**
 * Avoids rewriting an identical Wasmtime-mapped file on Windows. Changed components are
 * installed by rename so readers never observe partial bytes.
 */
const compositeBytes = fs.readFileSync(path.join(root, 'dist/antikythera-sdk.wasm'));
/** Ships the composed component alongside the Node delivery API; no compilation is needed at export time. */
const nodeDelivery = path.join(root, 'clients/npm/antikythera-sdk/delivery');
fs.mkdirSync(nodeDelivery, { recursive: true });
fs.writeFileSync(path.join(nodeDelivery, 'agent.wasm'), compositeBytes);
/** Python delivery uses the same browser tool loop, copied from the canonical JavaScript source. */
const pythonDelivery = path.join(root, 'clients/python/antikythera_agent/delivery_assets');
fs.mkdirSync(pythonDelivery, { recursive: true });
fs.copyFileSync(path.join(nodeDelivery, 'runtime.mjs'), path.join(pythonDelivery, 'runtime.mjs'));
fs.copyFileSync(path.join(nodeDelivery, 'client.d.ts'), path.join(pythonDelivery, 'client.d.ts'));
const pythonWasm = path.join(root, 'clients/python/antikythera_agent/antikythera.wasm');
if (!fs.existsSync(pythonWasm) || !fs.readFileSync(pythonWasm).equals(compositeBytes)) {
  const temporaryWasm = `${pythonWasm}.build-${process.pid}.tmp`;
  try {
    fs.writeFileSync(temporaryWasm, compositeBytes, { flag: 'wx' });
    fs.renameSync(temporaryWasm, pythonWasm);
  } finally {
    if (fs.existsSync(temporaryWasm)) fs.unlinkSync(temporaryWasm);
  }
}
const pythonComponent = path.join(root, 'clients/python/antikythera_agent/component');
for (const relative of ['', 'factory']) {
  const dir = path.join(pythonComponent, relative);
  if (!fs.existsSync(dir)) continue;
  for (const name of fs.readdirSync(dir)) {
    if (/^antikythera-sdk\..*\.wasm$/.test(name)) fs.unlinkSync(path.join(dir, name));
  }
}
fs.cpSync(component, pythonComponent, { recursive: true });
