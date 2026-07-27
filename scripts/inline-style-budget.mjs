#!/usr/bin/env node
// [TD-31] Ratchet на инлайн-стили фронтенда.
//
// Токен-дисциплина по ЦВЕТУ реально enforced (PostToolUse hook design-gate),
// а по типографике и spacing — нет: шкалы --t-11..28 и --s1..9 фактически
// advisory, потому что ничто не мешает написать fontSize: 13 или
// style={{ marginTop: 23 }}.
//
// Это не разовый рефактор — 170 и 612 вхождений не сводятся к токенам одним
// коммитом без риска. Поэтому: зафиксировать текущие числа, запретить рост,
// сводить по мере касания файлов. Ratchet опускается сам, когда числа
// уменьшаются (--update).
//
// Использование:
//   node scripts/inline-style-budget.mjs            # показать текущее
//   node scripts/inline-style-budget.mjs --check    # упасть при росте (CI)
//   node scripts/inline-style-budget.mjs --update   # опустить планку

import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SRC = join(ROOT, 'apps/desktop/src');
const BASELINE = join(ROOT, 'scripts/inline-style-baseline.json');

/** Тесты не считаем: инлайн-стиль в фикстуре ничего не говорит о продукте. */
const isCounted = (name) => name.endsWith('.tsx') && !name.endsWith('.test.tsx');

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (isCounted(entry)) out.push(p);
  }
  return out;
}

function count() {
  const perFile = {};
  let fontSize = 0;
  let inlineStyle = 0;
  for (const file of walk(SRC)) {
    const src = readFileSync(file, 'utf8');
    const a = (src.match(/fontSize:/g) ?? []).length;
    const b = (src.match(/style=\{\{/g) ?? []).length;
    if (a || b) perFile[relative(ROOT, file)] = { fontSize: a, inlineStyle: b };
    fontSize += a;
    inlineStyle += b;
  }
  return { fontSize, inlineStyle, perFile };
}

const mode = process.argv[2] ?? '';
const now = count();

if (mode === '--update') {
  writeFileSync(
    BASELINE,
    `${JSON.stringify({ fontSize: now.fontSize, inlineStyle: now.inlineStyle }, null, 2)}\n`,
  );
  console.log(`[inline-style] планка обновлена: fontSize=${now.fontSize} style=${now.inlineStyle}`);
  process.exit(0);
}

let base;
try {
  base = JSON.parse(readFileSync(BASELINE, 'utf8'));
} catch {
  console.error(`[inline-style] нет ${relative(ROOT, BASELINE)} — запусти с --update`);
  process.exit(2);
}

const grew = now.fontSize > base.fontSize || now.inlineStyle > base.inlineStyle;
const shrank = now.fontSize < base.fontSize || now.inlineStyle < base.inlineStyle;

console.log(
  `[inline-style] fontSize ${now.fontSize} (планка ${base.fontSize}) · ` +
    `style={{ ${now.inlineStyle} (планка ${base.inlineStyle})`,
);

if (mode === '--check') {
  if (grew) {
    console.error(
      '[inline-style] ОТКАЗ: инлайн-стилей стало больше.\n' +
        '  Используй шкалы var(--t-11..28) и var(--s1..9) из styles/tokens.css.\n' +
        '  Если рост осознан — обнови планку: node scripts/inline-style-budget.mjs --update',
    );
    process.exit(1);
  }
  if (shrank) {
    console.log('[inline-style] стало меньше — опусти планку: --update');
  }
  process.exit(0);
}

const top = Object.entries(now.perFile)
  .sort((a, b) => b[1].fontSize + b[1].inlineStyle - (a[1].fontSize + a[1].inlineStyle))
  .slice(0, 10);
console.log('\nТоп файлов:');
for (const [file, c] of top) {
  console.log(`  ${file.padEnd(56)} fontSize=${String(c.fontSize).padStart(3)} style=${c.inlineStyle}`);
}
