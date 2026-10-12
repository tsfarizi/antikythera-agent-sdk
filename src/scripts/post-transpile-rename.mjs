#!/usr/bin/env node
/**
 * Assigns descriptive core-module names to both jco namespace and instantiation bundles.
 * WIT exports identify application modules, Preview 1 exports identify WASI adapters,
 * and wit-component function names identify indirect-import trampolines. Import-table
 * fixups are paired with their trampolines by their WebAssembly type sections and
 * numbered function signatures. The cross-component canonical ABI adapter is identified
 * by its callee and transcoding imports. Original filenames and directory order never
 * determine a module's role. Unknown or ambiguous modules stop the build before mutation.
 * Usage: node src/scripts/post-transpile-rename.mjs [component-directory]
 */
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const coreFile = /^antikythera-sdk\..*\.core\d*\.wasm$|^antikythera-sdk\.core\d*\.wasm$/;
const wasmLiteral = /(['"])(\.\/)?(antikythera-sdk\.[a-z0-9.-]+\.wasm)\1/g;
const roles = ['runner', 'tool-registry', 'logic-hooks-passthrough'];

/** Reads an unsigned LEB128 section length from an already validated core module. */
function readUnsigned(bytes, cursor) {
  let value = 0;
  let shift = 0;
  let byte;
  do {
    byte = bytes[cursor.offset++];
    value += (byte & 0x7f) * 2 ** shift;
    shift += 7;
  } while (byte & 0x80);
  return value;
}

/** Returns the type section used to match a generated fixup to its import trampoline. */
function typeSection(bytes) {
  const cursor = { offset: 8 };
  while (cursor.offset < bytes.length) {
    const id = bytes[cursor.offset++];
    const size = readUnsigned(bytes, cursor);
    const section = bytes.subarray(cursor.offset, cursor.offset + size);
    if (id === 1) return section;
    cursor.offset += size;
  }
  throw new Error('generated import glue has no WebAssembly type section');
}

/** Validates every binary and records its actual import/export surface before planning renames. */
function inventory(directory) {
  return fs.readdirSync(directory).filter((name) => coreFile.test(name)).sort().map((name) => {
    const bytes = fs.readFileSync(path.join(directory, name));
    const module = new WebAssembly.Module(bytes);
    return {
      name, bytes,
      imports: WebAssembly.Module.imports(module),
      exports: WebAssembly.Module.exports(module),
      debugNames: WebAssembly.Module.customSections(module, 'name')
        .map((section) => Buffer.from(section).toString('utf8')).join('\n'),
    };
  });
}

/** Classifies module responsibilities without consulting jco's numbered filenames. */
function classify(records) {
  const assigned = new Map();
  const trampolines = [];
  const fixups = [];
  const assign = (record, role) => assigned.set(record, `antikythera-sdk.${role}.core.wasm`);
  for (const record of records) {
    const hasExport = (name, kind = 'function') => record.exports.some((entry) => entry.name === name && entry.kind === kind);
    const apiRoles = roles.filter((role) => record.exports.some((entry) => entry.kind === 'function'
      && entry.name.startsWith(`antikythera:agent-sdk/${role === 'logic-hooks-passthrough' ? 'logic-hooks' : role}@`)));
    if (apiRoles.length === 1) {
      assign(record, apiRoles[0]);
    } else if (apiRoles.length > 1) {
      throw new Error(`ambiguous WIT role in '${record.name}': ${apiRoles.join(', ')}`);
    } else if (hasExport('cabi_import_realloc') && record.imports.some((entry) => entry.module.startsWith('wasi:'))) {
      const owner = hasExport('clock_time_get') ? 'runner'
        : hasExport('random_get') ? 'tool-registry' : 'logic-hooks-passthrough';
      assign(record, `${owner}.wasi-preview1-adapter`);
    } else if (record.imports.length === 0 && hasExport('$imports', 'table')
      && record.exports.every((entry) => entry.name === '$imports' || (entry.kind === 'function' && /^\d+$/.test(entry.name)))) {
      const owner = record.debugNames.includes('indirect-antikythera:agent-sdk/runtime-hooks@') ? 'runner'
        : record.debugNames.includes('adapt-wasi_snapshot_preview1-random_get') ? 'tool-registry'
          : record.debugNames.includes('adapt-wasi_snapshot_preview1-fd_write') ? 'logic-hooks-passthrough' : null;
      if (!owner) throw new Error(`unknown import-trampoline owner in '${record.name}'; preserve wit-component name sections`);
      assign(record, `${owner}.import-trampolines`);
      trampolines.push(record);
    } else if (record.exports.length === 0 && record.imports.some((entry) => entry.module === '' && entry.name === '$imports' && entry.kind === 'table')
      && record.imports.every((entry) => entry.module === '' && (entry.name === '$imports' || (entry.kind === 'function' && /^\d+$/.test(entry.name))))) {
      fixups.push(record);
    } else if (record.imports.some((entry) => entry.module === 'callee')
      && record.imports.some((entry) => entry.module === 'transcode')
      && record.exports.length > 0 && record.exports.every((entry) => entry.kind === 'function' && /^adapter\d+$/.test(entry.name))) {
      assign(record, 'component-abi-adapters');
    } else {
      throw new Error(`unknown core-module role in '${record.name}'; update classification for the new component shape`);
    }
  }
  for (const record of fixups) {
    const names = record.imports.filter((entry) => entry.kind === 'function').map((entry) => entry.name).sort();
    const candidates = trampolines.filter((shim) => {
      const exported = shim.exports.filter((entry) => entry.kind === 'function').map((entry) => entry.name).sort();
      return JSON.stringify(names) === JSON.stringify(exported) && typeSection(record.bytes).equals(typeSection(shim.bytes));
    });
    if (candidates.length !== 1) {
      throw new Error(`ambiguous import-fixup owner in '${record.name}': ${candidates.length} matching trampolines`);
    }
    assigned.set(record, assigned.get(candidates[0]).replace('.import-trampolines.', '.import-fixups.'));
  }
  return assigned;
}

/** Checks filename collisions and loader completeness before changing either files or references. */
function plan(directory, records, assigned) {
  const targets = new Set();
  const renameMap = new Map();
  for (const record of records) {
    const target = assigned.get(record);
    if (targets.has(target)) throw new Error(`rename collision: multiple modules implement '${target}'`);
    if (target !== record.name && fs.existsSync(path.join(directory, target))) {
      throw new Error(`rename collision: target '${target}' already exists`);
    }
    targets.add(target);
    renameMap.set(record.name, target);
  }
  const loaderPath = path.join(directory, 'antikythera-sdk.js');
  const source = fs.readFileSync(loaderPath, 'utf8');
  const referenced = new Set();
  const rewritten = source.replace(wasmLiteral, (literal, quote, prefix = '', name) => {
    const target = renameMap.get(name);
    if (!target) throw new Error(`loader references unclassified module '${name}'`);
    referenced.add(name);
    return `${quote}${prefix}${target}${quote}`;
  });
  for (const record of records) {
    if (!referenced.has(record.name)) throw new Error(`core module '${record.name}' has no loader reference`);
  }
  return { loaderPath, source, rewritten };
}

/** Stages the loader and rolls back renamed binaries if installing the new loader fails. */
function apply(directory, records, assigned, loader) {
  const temporary = `${loader.loaderPath}.rename-${process.pid}.tmp`;
  const moved = [];
  try {
    if (loader.rewritten !== loader.source) fs.writeFileSync(temporary, loader.rewritten, { flag: 'wx' });
    for (const record of records) {
      const target = assigned.get(record);
      if (target === record.name) continue;
      fs.renameSync(path.join(directory, record.name), path.join(directory, target));
      moved.push([record.name, target]);
    }
    if (loader.rewritten !== loader.source) fs.renameSync(temporary, loader.loaderPath);
  } catch (error) {
    for (const [original, target] of moved.reverse()) fs.renameSync(path.join(directory, target), path.join(directory, original));
    throw error;
  } finally {
    if (fs.existsSync(temporary)) fs.unlinkSync(temporary);
  }
  return moved.length;
}

try {
  const directory = process.argv[2] ? path.resolve(process.argv[2]) : path.join(root, 'clients/npm/antikythera-sdk/component');
  const records = inventory(directory);
  if (records.length === 0) throw new Error(`no core modules found in '${directory}'; transpile the component first`);
  const assigned = classify(records);
  const loader = plan(directory, records, assigned);
  const count = apply(directory, records, assigned, loader);
  for (const record of records) console.log(`${record.name} -> ${assigned.get(record)}`);
  console.log(`post-transpile-rename: ${records.length} modules classified, ${count} renamed`);
} catch (error) {
  console.error(`post-transpile-rename: ${error.message}`);
  process.exitCode = 1;
}
