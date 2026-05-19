#!/usr/bin/env node
// M11.5 паспорта: версия должна быть синхронной в Cargo.toml десктопа,
// tauri.conf.json и обоих package.json (корень + apps/desktop).
// CI падает при рассинхроне.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

const desktopCargo = readFileSync(join(root, 'apps/desktop/src-tauri/Cargo.toml'), 'utf8');
const tauriConf = JSON.parse(
  readFileSync(join(root, 'apps/desktop/src-tauri/tauri.conf.json'), 'utf8'),
);
const desktopPkg = JSON.parse(readFileSync(join(root, 'apps/desktop/package.json'), 'utf8'));
const rootPkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));

const cargoMatch = desktopCargo.match(/^version\s*=\s*"([^"]+)"/m);
if (!cargoMatch) {
  console.error('Failed to parse version from apps/desktop/src-tauri/Cargo.toml');
  process.exit(1);
}

const versions = {
  'apps/desktop/src-tauri/Cargo.toml': cargoMatch[1],
  'apps/desktop/src-tauri/tauri.conf.json': tauriConf.version,
  'apps/desktop/package.json': desktopPkg.version,
  'package.json': rootPkg.version,
};

const unique = new Set(Object.values(versions));
if (unique.size > 1) {
  console.error('Version mismatch (M11.5):');
  for (const [file, version] of Object.entries(versions)) {
    console.error(`  ${version}\t${file}`);
  }
  process.exit(1);
}

const [v] = unique;
console.log(`All versions match: ${v}`);
