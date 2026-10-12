#!/usr/bin/env node
/**
 * Pack-smoke — publish gate for the npm and python packages. Resolves every file the packages
 * PROMISE to ship (exports map including browser/default/node conditions, files array,
 * main/types) against the working tree, then builds a python wheel and asserts the PEP 561
 * marker, the inline stub, the WASM artifact, and the jco component bundle are all inside it.
 * Exit 0 only when everything resolves; any gap exits 1 with an explicit message so
 * `prepublishOnly` fails closed. Node >= 18, ESM, zero dependencies.
 */

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const pkgDir = path.join(repoRoot, 'clients', 'npm', 'antikythera-sdk');
const pythonCmd = process.platform === 'win32' ? 'python' : 'python3';

const failures = [];

/** Enumerates every emitted core module so distribution checks cannot accept a partial bundle. */
const requiredCoreFiles = ['', 'factory'].flatMap((relative) => {
  const directory = path.join(pkgDir, 'component', relative);
  const loader = fs.readFileSync(path.join(directory, 'antikythera-sdk.js'), 'utf8');
  const names = [...new Set([...loader.matchAll(/antikythera-sdk\.[a-z0-9.-]+\.wasm/g)].map((match) => match[0]))];
  if (names.length === 0) fail(`component/${relative}: loader references no core modules`);
  return names.map((name) => {
    if (/^antikythera-sdk\.core\d*\.wasm$/.test(name)) fail(`component/${relative}: opaque core-module name ${name}`);
    if (!fs.existsSync(path.join(directory, name))) fail(`component/${relative}: loader module missing ${name}`);
    return ['component', relative, name].filter(Boolean).join('/');
  });
});

function fail(message) {
  failures.push(message);
}

function checkTarget(relSpec, source) {
  const rel = relSpec.replace(/^\.\//, '');
  if (rel.includes('..')) {
    fail(`${source}: target escapes package dir: ${relSpec}`);
    return;
  }
  const abs = path.join(pkgDir, rel);
  let stat;
  try {
    stat = fs.statSync(abs);
  } catch {
    fail(`${source}: missing on disk: ${rel}`);
    return;
  }
  if (stat.isDirectory()) {
    let entries;
    try {
      entries = fs.readdirSync(abs);
    } catch (e) {
      fail(`${source}: cannot read directory ${rel}: ${e.message}`);
      return;
    }
    if (entries.length === 0) {
      fail(`${source}: directory is empty: ${rel}`);
    }
  }
}

function collectExportTargets(node, out) {
  if (typeof node === 'string') {
    out.add(node);
    return;
  }
  if (node && typeof node === 'object') {
    /**
     * Recurses through every condition key: browser/default/node and future ones.
     */
    for (const value of Object.values(node)) collectExportTargets(value, out);
  }
}


const manifest = JSON.parse(fs.readFileSync(path.join(pkgDir, 'package.json'), 'utf8'));

if (manifest.main) checkTarget(manifest.main, 'main');
if (manifest.types) checkTarget(manifest.types, 'types');
for (const entry of manifest.files ?? []) checkTarget(entry, `files[${entry}]`);

const exportTargets = new Set();
collectExportTargets(manifest.exports, exportTargets);
for (const target of exportTargets) checkTarget(target, `exports[${target}]`);

/**
 * Shell:true with a single literal command string: npm is a .cmd shim on Windows that cannot be
 * spawned directly; the string carries no interpolated input, so the DEP0190 concatenation
 * concern does not apply.
 */
const pack = spawnSync('npm pack --dry-run --json', {
  cwd: pkgDir,
  encoding: 'utf8',
  shell: true,
});
if (pack.status !== 0 || !pack.stdout.trim()) {
  fail(`npm pack --dry-run --json failed:\n${pack.stderr || pack.stdout}`);
} else {
  let packedFiles;
  try {
    packedFiles = JSON.parse(pack.stdout)[0].files.map((f) => f.path);
  } catch (e) {
    fail(`cannot parse npm pack --json output: ${e.message}`);
    packedFiles = [];
  }
  for (const name of ['component/instantiate.js', 'component/instantiate.d.ts', 'component/factory/antikythera-sdk.js',
    'delivery/index.js', 'delivery/index.d.ts', 'delivery/client.d.ts', 'delivery/runtime.mjs', 'delivery/agent.wasm']) {
    if (!packedFiles.includes(name)) fail(`npm tarball: missing ${name}`);
  }
  for (const name of requiredCoreFiles) {
    if (!packedFiles.includes(name)) fail(`npm tarball: missing ${name}`);
  }
  for (const name of packedFiles.filter((name) => name.startsWith('component/') && name.endsWith('.wasm'))) {
    if (!requiredCoreFiles.includes(name)) fail(`npm tarball: unreferenced core module ${name}`);
  }
  console.log(`npm tarball would carry ${packedFiles.length} entries`);
}

/**
 * Both distributions must expose identical client bindings and core modules.
 */
const componentDir = path.join(pkgDir, 'component');
const pythonComponentDir = path.join(repoRoot, 'clients/python/antikythera_agent/component');
function checkBundleDirectory(relative = '') {
  for (const entry of fs.readdirSync(path.join(componentDir, relative), { withFileTypes: true })) {
    const name = path.join(relative, entry.name);
    if (entry.isDirectory()) { checkBundleDirectory(name); continue; }
    try {
      if (!fs.readFileSync(path.join(componentDir, name)).equals(fs.readFileSync(path.join(pythonComponentDir, name)))) {
        fail(`component bundle differs between npm and Python: ${name}`);
      }
    } catch (error) { fail(`Python component bundle missing ${name}: ${error.message}`); }
  }
}
checkBundleDirectory();
/** The delivery APIs must ship one composite and one shared browser loop across both distributions. */
for (const [nodeFile, pythonFile] of [['delivery/agent.wasm', 'antikythera.wasm'],
  ['delivery/runtime.mjs', 'delivery_assets/runtime.mjs'], ['delivery/client.d.ts', 'delivery_assets/client.d.ts']]) {
  if (!fs.readFileSync(path.join(pkgDir, nodeFile)).equals(fs.readFileSync(path.join(repoRoot, 'clients/python/antikythera_agent', pythonFile)))) {
    fail(`delivery artifact differs between npm and Python: ${nodeFile}`);
  }
}


const pyTmp = fs.mkdtempSync(path.join(os.tmpdir(), 'pack-smoke-py-'));
/** Builds from a clean source snapshot so setuptools cannot retain obsolete generated filenames. */
const pythonSource = path.join(repoRoot, 'clients/python');
const pythonBuildSource = path.join(pyTmp, 'source');
fs.cpSync(pythonSource, pythonBuildSource, {
  recursive: true,
  filter: (source) => {
    const relative = path.relative(pythonSource, source);
    if (!relative) return true;
    const parts = relative.split(path.sep);
    if (parts[0] === 'antikythera_agent') return !parts.includes('__pycache__');
    return parts.length === 1 && fs.statSync(source).isFile();
  },
});
const wheelBuild = spawnSync(
  pythonCmd,
  ['-m', 'pip', 'wheel', '--no-deps', '--no-build-isolation', '-w', pyTmp, pythonBuildSource],
  {
    cwd: repoRoot,
    encoding: 'utf8',
  },
);
if (wheelBuild.status !== 0) {
  console.error('pack-smoke: python wheel build failed (python tooling required):');
  console.error(wheelBuild.stderr || wheelBuild.stdout);
  process.exit(1);
}

const wheels = fs.readdirSync(pyTmp).filter((f) => f.endsWith('.whl'));
if (wheels.length !== 1) {
  fail(`expected exactly one wheel in tmp dir, got: ${wheels.join(', ') || '(none)'}`);
} else {
  const listZip = spawnSync(
    pythonCmd,
    [
      '-c',
      'import sys, zipfile; print("\\n".join(zipfile.ZipFile(sys.argv[1]).namelist()))',
      path.join(pyTmp, wheels[0]),
    ],
    { encoding: 'utf8' },
  );
  if (listZip.status !== 0) {
    console.error('pack-smoke: could not inspect wheel contents (python tooling required):');
    console.error(listZip.stderr || listZip.stdout);
    process.exit(1);
  }
  const names = new Set(listZip.stdout.split('\n').map((n) => n.trim()).filter(Boolean));
  for (const required of [
    'antikythera_agent/py.typed',
    'antikythera_agent/__init__.pyi',
    'antikythera_agent/antikythera.wasm',
    'antikythera_agent/component/instantiate.js',
    'antikythera_agent/component/factory/antikythera-sdk.js',
    'antikythera_agent/delivery.py',
    'antikythera_agent/delivery_assets/runtime.mjs',
    'antikythera_agent/delivery_assets/client.d.ts',
    ...requiredCoreFiles.map((name) => `antikythera_agent/${name}`),
  ]) {
    if (!names.has(required)) fail(`wheel ${wheels[0]}: missing ${required}`);
  }
  const hasComponent = [...names].some((n) => n.startsWith('antikythera_agent/component/'));
  if (!hasComponent) fail(`wheel ${wheels[0]}: missing antikythera_agent/component/ bundle`);
  for (const name of [...names].filter((name) => name.startsWith('antikythera_agent/component/') && name.endsWith('.wasm'))) {
    if (!requiredCoreFiles.includes(name.slice('antikythera_agent/'.length))) {
      fail(`wheel ${wheels[0]}: unreferenced core module ${name}`);
    }
  }
  console.log(`python wheel ${wheels[0]} carries ${names.size} entries`);
}


fs.rmSync(pyTmp, { recursive: true, force: true });

if (failures.length > 0) {
  console.error('pack-smoke FAILED:');
  for (const message of failures) console.error(`  - ${message}`);
  process.exit(1);
}
console.log('pack-smoke OK: every packaged target resolves');
