#!/usr/bin/env node
/**
 * Builds the mdBook site from a staged source tree.
 *
 * `book.toml` uses `src = "."` so chapters can live at the repository root, but
 * the HTML renderer copies every non-markdown file under `src` into the site.
 * Built from the repository root that copies `.git/`, `target/`, `node_modules/`,
 * and every client bundle into the published artifact, and a bare `mdbook build`
 * on a checkout with restricted files (e.g. Windows ACLs on generated caches)
 * fails mid-copy. Staging only the book inputs keeps the site to book files and
 * makes the build independent of unrelated repository state.
 *
 * Usage: node src/scripts/build-docs.mjs [--serve]
 */
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const stage = path.join(root, '.runtime-build', 'book-src');
const inputs = ['book.toml', 'SUMMARY.md', 'README.md', 'documentation', 'theme'];

fs.rmSync(stage, { recursive: true, force: true });
fs.mkdirSync(stage, { recursive: true });
for (const input of inputs) {
  fs.cpSync(path.join(root, input), path.join(stage, input), { recursive: true });
}

const serve = process.argv.includes('--serve');
const result = spawnSync('mdbook', [serve ? 'serve' : 'build', stage], { stdio: 'inherit' });
if (result.error) {
  console.error(`mdbook could not be executed: ${result.error.message}`);
  console.error('Install it with `cargo install mdbook` or the taiki-e install action.');
  process.exit(1);
}
process.exit(result.status ?? 1);
