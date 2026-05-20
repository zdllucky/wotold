#!/usr/bin/env node
// PostToolUse TDD-warn hook.
// При правке source-файла без соседнего теста — выводит WARN в stderr.
// Не блокирует (exit 0). Цель — напомнить о TDD-правиле без overhead'а
// автоматического запуска тестов на каждую правку.
//
// Эвристика «соседнего теста»:
//   foo/bar.rs       → есть #[cfg(test)] mod tests внутри foo/bar.rs
//   foo/bar.tsx      → файл foo/bar.test.tsx (или .test.ts) рядом
//   foo/bar.ts       → файл foo/bar.test.ts рядом
// Файлы без соседнего теста — кандидаты на дописывание тестов.
//
// Игнорируем:
//   - сами тест-файлы (*.test.ts, *.test.tsx, _test.rs)
//   - типы-only (*.d.ts)
//   - UI-pure render: pages/*, App.tsx, main.tsx, mock'и
//   - конфиги: *.config.ts, vite-env.d.ts

import { readFileSync, existsSync } from 'node:fs';
import { basename, dirname, extname, join } from 'node:path';

const stdin = await new Promise((r) => {
  let buf = '';
  process.stdin.on('data', (c) => (buf += c));
  process.stdin.on('end', () => r(buf));
});

let payload;
try {
  payload = JSON.parse(stdin || '{}');
} catch {
  process.exit(0);
}

const file = payload?.tool_input?.file_path;
if (!file) process.exit(0);

const ext = extname(file);
const base = basename(file);

// Skip non-source.
if (!['.rs', '.ts', '.tsx'].includes(ext)) process.exit(0);
if (base.endsWith('.d.ts')) process.exit(0);
if (base.endsWith('.test.ts') || base.endsWith('.test.tsx')) process.exit(0);
if (base.endsWith('_test.rs') || base === 'tests.rs') process.exit(0);

// Skip non-testable surfaces.
const SKIP_GLOB = [
  '/pages/',
  '/styles/',
  '/main.tsx',
  '/App.tsx',
  '/dev-tauri-mock.ts',
  '/vite-env.d.ts',
  '/api/', // тонкие Tauri-обёртки — тестируются через integration
];
if (SKIP_GLOB.some((g) => file.includes(g))) process.exit(0);
if (base.endsWith('.config.ts')) process.exit(0);

function hasInlineRustTest(rsPath) {
  try {
    const src = readFileSync(rsPath, 'utf8');
    return /#\[cfg\(test\)\]\s*mod\s+tests/.test(src);
  } catch {
    return false;
  }
}

function hasSiblingTest(file) {
  const dir = dirname(file);
  const stem = base.replace(ext, '');
  const candidates =
    ext === '.tsx'
      ? [`${stem}.test.tsx`, `${stem}.test.ts`]
      : ext === '.ts'
        ? [`${stem}.test.ts`]
        : [];
  return candidates.some((c) => existsSync(join(dir, c)));
}

let missing = false;
if (ext === '.rs') {
  missing = !hasInlineRustTest(file);
} else {
  missing = !hasSiblingTest(file);
}

if (missing) {
  const rel = file.replace(process.env.CLAUDE_PROJECT_DIR ?? '', '').replace(/^\//, '');
  process.stderr.write(
    `[tdd-warn] ${rel} — нет соседнего теста.\n` +
      `  Правило: новые/изменённые модули обязаны покрываться unit-тестами (ECC testing.md, 80%).\n` +
      `  Rust: добавить #[cfg(test)] mod tests внутри файла.\n` +
      `  TS:   создать ${base.replace(ext, `.test${ext}`)} рядом.\n`,
  );
}

process.exit(0);
