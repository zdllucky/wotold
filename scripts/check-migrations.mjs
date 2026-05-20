#!/usr/bin/env node
// scripts/check-migrations.mjs
//
// Проверяет SQLx migrations: имена матчат NNNN_<name>.sql, номера
// последовательны от 0001 без пропусков и дубликатов. На любое нарушение
// process.exit(1) c понятным сообщением.
//
// Бежит в CI (см. .github/workflows/ci.yml) и локально:
//     node scripts/check-migrations.mjs

import { readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const MIGRATIONS_DIR = path.resolve(__dirname, '..', 'apps/desktop/src-tauri/migrations');
const PATTERN = /^(\d{4})_[a-z0-9_]+\.sql$/;

function main() {
  let entries;
  try {
    entries = readdirSync(MIGRATIONS_DIR);
  } catch (err) {
    console.error(`[check-migrations] cannot read ${MIGRATIONS_DIR}: ${err.message}`);
    process.exit(2);
  }

  const files = entries
    .filter((name) => statSync(path.join(MIGRATIONS_DIR, name)).isFile())
    .filter((name) => name.endsWith('.sql'));

  if (files.length === 0) {
    console.error('[check-migrations] no .sql migrations found');
    process.exit(1);
  }

  files.sort();

  const errors = [];
  const seen = new Set();
  let expected = 1;

  for (const name of files) {
    const m = PATTERN.exec(name);
    if (!m) {
      errors.push(`bad filename: '${name}' (must match NNNN_<snake_name>.sql)`);
      continue;
    }
    const num = Number(m[1]);
    if (seen.has(num)) {
      errors.push(`duplicate migration number ${num} in '${name}'`);
      continue;
    }
    seen.add(num);
    if (num !== expected) {
      errors.push(`out-of-order: expected ${String(expected).padStart(4, '0')}_*, got '${name}'`);
    }
    expected += 1;
  }

  if (errors.length > 0) {
    console.error('[check-migrations] FAILED:');
    for (const e of errors) console.error(`  - ${e}`);
    process.exit(1);
  }

  console.log(`[check-migrations] OK: ${files.length} migration(s) sequential from 0001 to ${String(files.length).padStart(4, '0')}`);
  for (const name of files) console.log(`  ✓ ${name}`);
}

main();
