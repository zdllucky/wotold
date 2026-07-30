// [B34.4] Реестр настроек обязан совпадать с тем, что реально нарисовано.
//
// Расхождение здесь не падает и не светится: палитра просто выдаст пункт с
// сырым ключом вместо подписи или уведёт в раздел, где искомой строки нет.
// Поэтому проверки статические — по локали и по исходникам страницы.

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, test } from 'vitest';

import { ru } from '../i18n/ru';

import {
  SECTION_ICONS,
  SECTION_LABEL_KEYS,
  SECTION_ORDER,
  SETTINGS_ENTRIES,
  type SectionId,
} from './settingsIndex';

const SRC = join(process.cwd(), 'src');

/** Достать значение по точечному пути, как это делает `t()`. */
function lookup(key: string): unknown {
  return key
    .split('.')
    .reduce<unknown>(
      (acc, part) =>
        acc && typeof acc === 'object' ? (acc as Record<string, unknown>)[part] : undefined,
      ru,
    );
}

function tsxFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) return tsxFiles(full);
    return full.endsWith('.tsx') && !full.endsWith('.test.tsx') ? [full] : [];
  });
}

const allSource = tsxFiles(SRC)
  .map((f) => readFileSync(f, 'utf8'))
  .join('\n');

describe('settings index', () => {
  test('every label key exists in the base locale', () => {
    const missing = SETTINGS_ENTRIES.filter((e) => typeof lookup(e.labelKey) !== 'string');
    expect(missing.map((e) => `${e.id} → ${e.labelKey}`)).toEqual([]);
  });

  test('every section label key exists too', () => {
    const missing = Object.entries(SECTION_LABEL_KEYS).filter(
      ([, key]) => typeof lookup(key) !== 'string',
    );
    expect(missing.map(([id, key]) => `${id} → ${key}`)).toEqual([]);
  });

  test('anchors are unique', () => {
    const ids = SETTINGS_ENTRIES.map((e) => e.id);
    expect(ids.length).toBe(new Set(ids).size);
  });

  test('sections cover the full set, in one canonical order', () => {
    const fromOrder = [...SECTION_ORDER].sort();
    const fromIcons = (Object.keys(SECTION_ICONS) as SectionId[]).sort();
    const fromLabels = (Object.keys(SECTION_LABEL_KEYS) as SectionId[]).sort();
    expect(fromOrder).toEqual(fromIcons);
    expect(fromOrder).toEqual(fromLabels);
    expect(SECTION_ORDER.length).toBe(new Set(SECTION_ORDER).size);
  });

  test('every entry points at a section that exists', () => {
    const known = new Set<string>(SECTION_ORDER);
    const orphans = SETTINGS_ENTRIES.filter((e) => !known.has(e.section));
    expect(orphans.map((e) => e.id)).toEqual([]);
  });

  test('every label key is actually rendered somewhere in the UI', () => {
    // Ловит и опечатку в ключе, и строку, удалённую со страницы, но забытую
    // в реестре: палитра бы уводила в раздел, где искать уже нечего.
    const unused = SETTINGS_ENTRIES.filter((e) => !allSource.includes(`'${e.labelKey}'`));
    expect(unused.map((e) => `${e.id} → ${e.labelKey}`)).toEqual([]);
  });
});
