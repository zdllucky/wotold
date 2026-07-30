// [B32.1] Статический guard: каждая шапка страницы таскает окно.
//
// В Tauri 2 перетаскивание окна решает ровно один атрибут —
// `data-tauri-drag-region`. CSS `-webkit-app-region` там no-op, а drag-скрипт,
// не найдя атрибут по всей цепочке предков, просто отвечает «не тащим». Ловить
// это рендером бесполезно: jsdom не таскает окна, и тест зеленел бы на
// сломанном приложении.
//
// Поэтому проверка статическая — по исходникам. Она переживёт любые будущие
// страницы: `ViewHead` атрибут несёт, но пара страниц рисует `.view-head`
// руками (у них breadcrumb и кнопка «назад» вместо иконки с заголовком), и
// ровно там его однажды и забыли — в Настройках.

import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, test } from 'vitest';

// `import.meta.url` под jsdom-окружением не file-схема, поэтому идём от корня
// прогона: vitest запускается из `apps/desktop`.
const SRC = join(process.cwd(), 'src');

function tsxFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) return tsxFiles(full);
    return full.endsWith('.tsx') ? [full] : [];
  });
}

interface Offender {
  file: string;
  line: number;
  text: string;
}

function headsWithoutDrag(): Offender[] {
  const out: Offender[] = [];
  for (const file of tsxFiles(SRC)) {
    if (file.endsWith('.test.tsx')) continue;
    const lines = readFileSync(file, 'utf8').split('\n');
    lines.forEach((text, i) => {
      if (!text.includes('className="view-head"')) return;
      if (text.includes('data-tauri-drag-region')) return;
      out.push({ file: file.slice(SRC.length + 1), line: i + 1, text: text.trim() });
    });
  }
  return out;
}

describe('view-head drag region', () => {
  test('every hand-rolled .view-head carries the drag attribute', () => {
    const offenders = headsWithoutDrag();
    expect(
      offenders,
      `шапки без data-tauri-drag-region — окно за них не тащится:\n${offenders
        .map((o) => `  ${o.file}:${o.line}  ${o.text}`)
        .join('\n')}`,
    ).toEqual([]);
  });

  test('the guard actually inspects something', () => {
    expect(existsSync(SRC), `не нашли исходники в ${SRC}`).toBe(true);
    // Страховка от «зелено, потому что ничего не нашли»: если разметка шапок
    // переедет на другой класс, тест выше замолчит, и его молчание надо
    // отличать от реальной проверки.
    const found = tsxFiles(SRC)
      .filter((f) => !f.endsWith('.test.tsx'))
      .flatMap((f) => readFileSync(f, 'utf8').split('\n'))
      .filter((l) => l.includes('className="view-head"'));
    expect(found.length).toBeGreaterThanOrEqual(3);
  });
});
