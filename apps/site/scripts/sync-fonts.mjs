#!/usr/bin/env node
// Копирует self-hosted шрифты из десктопа в public/ сайта.
//
// Почему копия, а не второй набор файлов в git: источник истины по шрифтам —
// apps/desktop/public/fonts/ (TD-28: раньше стили тянули @import с
// fonts.googleapis.com, что для продукта, который продаётся приватностью, было
// fingerprint-утечкой). Сайт держит ту же типографику и то же обещание «ноль
// запросов на сторонние хосты», но дублировать 16 .woff2 в репозитории —
// значит завести второй набор, который разъедется при следующем обновлении
// шрифта. Поэтому копия делается на prebuild, а apps/site/public/fonts/ лежит
// в .gitignore.
//
// Процедура обновления самих файлов — apps/desktop/public/fonts/README.md.

import { cp, mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const siteRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const DESKTOP = join(siteRoot, '..', 'desktop');
const SRC = join(DESKTOP, 'public', 'fonts');
const SRC_CSS = join(DESKTOP, 'src', 'styles', 'fonts.css');
const DEST = join(siteRoot, 'public', 'fonts');

async function main() {
  let entries;
  try {
    entries = await readdir(SRC);
  } catch {
    console.error(
      `[sync-fonts] Источник не найден: ${SRC}\n` +
        `[sync-fonts] Шрифты живут в десктопе; без них сайт соберётся, но упадёт в системную типографику.`,
    );
    process.exit(1);
  }

  const woff2 = entries.filter((name) => name.endsWith('.woff2'));
  if (woff2.length === 0) {
    console.error(`[sync-fonts] В ${SRC} нет ни одного .woff2`);
    process.exit(1);
  }

  // Чистим каталог целиком: иначе переименованный в десктопе файл остаётся
  // здесь навсегда и продолжает раздаваться.
  await rm(DEST, { recursive: true, force: true });
  await mkdir(DEST, { recursive: true });

  let bytes = 0;
  for (const name of woff2) {
    const from = join(SRC, name);
    await cp(from, join(DEST, name));
    bytes += (await stat(from)).size;
  }

  // @font-face-декларации берутся из десктопа же, но пути переписываются на
  // относительные. В десктопе Vite отдаёт public/ по корню, поэтому там
  // `url('/fonts/x.woff2')`. У сайта base = /wotold, и абсолютный путь ушёл бы
  // мимо. Относительный `./x.woff2` резолвится от самого стилевого файла и
  // работает при любом base — включая будущий переезд на собственный домен.
  const css = await readFile(SRC_CSS, 'utf8');
  const rewritten = css.replace(/url\((['"]?)\/fonts\//g, 'url($1./');
  const missed = rewritten.match(/url\(['"]?\//g);
  if (missed) {
    console.error(
      `[sync-fonts] В fonts.css остались абсолютные url() (${missed.length}) — ` +
        `под base они дадут 404. Проверь apps/desktop/src/styles/fonts.css.`,
    );
    process.exit(1);
  }

  const referenced = [...rewritten.matchAll(/url\(['"]?\.\/([^'")]+)\)?/g)].map((m) =>
    m[1].replace(/['"]$/, ''),
  );
  const absent = referenced.filter((name) => !woff2.includes(name));
  if (absent.length > 0) {
    console.error(`[sync-fonts] fonts.css ссылается на отсутствующие файлы: ${absent.join(', ')}`);
    process.exit(1);
  }

  await writeFile(
    join(DEST, 'fonts.css'),
    `/* СГЕНЕРИРОВАНО apps/site/scripts/sync-fonts.mjs — не редактировать.\n` +
      `   Источник: apps/desktop/src/styles/fonts.css (пути переписаны на относительные). */\n\n` +
      rewritten,
  );

  console.log(
    `[sync-fonts] ${woff2.length} файлов, ${Math.round(bytes / 1024)} КБ + fonts.css → public/fonts/`,
  );
}

await main();
