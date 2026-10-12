/**
 * Test-only materialization of a server-served jco bundle. Node imports the fetched bytes from a
 * local ESM tree because its native loader rejects HTTP module URLs. The caller owns and removes
 * tempDir after importing the bundle. preload() follows static relative imports and pre-fetches
 * referenced WASM.
 */

'use strict';

import { pathToFileURL } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';

/** True bila URL absolut berada di bawah `baseUrl` (origin + path prefix). */
function isUnderBase(candidateHref, baseHref) {
  const base = new URL(baseHref);
  const candidate = new URL(candidateHref);
  if (candidate.origin !== base.origin) return false;
  const basePath = base.pathname.endsWith('/') ? base.pathname : `${base.pathname}/`;
  return candidate.pathname.startsWith(basePath);
}

/**
 * Instal loader materialisasi bundle http -> temp dir.
 * @param {object} options
 * @param {string} options.baseUrl - base URL bundle yang di-serve server
 *   (contoh `http://127.0.0.1:PORT/antikythera/v1/component/`).
 * @param {string} options.tempDir - direktori temp tempat file dimaterialisasi.
 * @returns {{ registry: { fetches: Array<object> }, preload: Function, deregister: Function }}
 */
export function installHttpBundleLoader({ baseUrl, tempDir }) {
  if (typeof baseUrl !== 'string' || !baseUrl) {
    throw new Error('http-bundle-loader: baseUrl is required');
  }
  if (typeof tempDir !== 'string' || !tempDir) {
    throw new Error('http-bundle-loader: tempDir is required');
  }
  const base = new URL(baseUrl);
  const basePath = base.pathname.endsWith('/') ? base.pathname : `${base.pathname}/`;
  const fileCache = new Map(); /** HttpHref -> absolute file path (sudah dimaterialisasi). */
  const fetches = []; /** {url, status, contentType, bytes, kind}. */


  function httpHrefToFilePath(httpHref) {
    const parsed = new URL(httpHref);
    if (parsed.origin !== base.origin) {
      throw new Error(`http-bundle-loader: fetch outside served origin: ${httpHref}`);
    }
    if (!parsed.pathname.startsWith(basePath)) {
      throw new Error(`http-bundle-loader: fetch outside served base ${basePath}: ${httpHref}`);
    }
    const rel = parsed.pathname.slice(basePath.length);
    if (!rel || rel.split('/').some((seg) => seg === '..' || seg === '' || seg === '.')) {
      throw new Error(`http-bundle-loader: unsafe relative path '${rel}' in ${httpHref}`);
    }
    const filePath = path.join(tempDir, ...rel.split('/'));
    const root = path.resolve(tempDir);
    if (!path.resolve(filePath).startsWith(root + path.sep)) {
      throw new Error(`http-bundle-loader: escaped temp dir: ${httpHref}`);
    }
    return filePath;
  }


  async function materialize(httpHref) {
    const cached = fileCache.get(httpHref);
    if (cached) return cached;

    const filePath = httpHrefToFilePath(httpHref);
    let response;
    try {
      response = await fetch(httpHref);
    } catch (err) {
      throw new Error(
        `http-bundle-loader: fetch ${httpHref} failed: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
    const bytes = Buffer.from(await response.arrayBuffer());
    if (!response.ok) {
      throw new Error(`http-bundle-loader: fetch ${httpHref} failed (HTTP ${response.status})`);
    }
    await fs.promises.mkdir(path.dirname(filePath), { recursive: true });
    await fs.promises.writeFile(filePath, bytes);

    const kind = httpHref.endsWith('.wasm') ? 'wasm' : 'js';
    fetches.push({
      url: httpHref,
      status: response.status,
      contentType: response.headers.get('content-type'),
      bytes: bytes.length,
      kind,
    });
    fileCache.set(httpHref, filePath);

    /**
     * Bundle jco di Node membaca WASM via fs.readFile(import.meta.url-rel). Pre-fetch asset
     * `.wasm` yang dirujuk source agar path file: ter-resolve.
     */
    if (kind === 'js') {
      const source = bytes.toString('utf8');
      for (const match of source.matchAll(/(?:\bfrom\s*|\bimport\s*)['"](\.{1,2}\/[^'"]+)['"]/g)) {
        const dependency = new URL(match[1], httpHref).href;
        if (!isUnderBase(dependency, base.href)) {
          throw new Error(`http-bundle-loader: import escapes served base: ${dependency}`);
        }
        await materialize(dependency);
      }
      const wasmRefs = [...new Set(source.match(/[A-Za-z0-9_./-]+\.wasm/g) ?? [])];
      for (const ref of wasmRefs) {
        const wasmHref = new URL(ref, httpHref).href;
        if (isUnderBase(wasmHref, base.href)) {
          await materialize(wasmHref);
        }
      }
    }
    return filePath;
  }

  return {
    registry: { fetches },
    async preload(entryHref) {
      await fs.promises.mkdir(tempDir, { recursive: true });
      await fs.promises.writeFile(path.join(tempDir, 'package.json'), '{"type":"module"}');
      return pathToFileURL(await materialize(entryHref)).href;
    },
    deregister() {}, /** No process-wide loader hooks are installed. */
  };
}
