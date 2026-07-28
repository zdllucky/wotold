#!/usr/bin/env node
// Гейт собранного сайта. Две проверки:
//
//   1. Ничего не грузится со сторонних хостов.
//   2. Все внутренние ссылки ведут на существующие страницы.
//
// Вторая появилась не умозрительно: относительная ссылка `../consent/` со
// страницы `/legal/terms/` уводит в `/legal/consent/`, которого нет. Astro
// такие ссылки не проверяет, глазами в трёх локалях это не ловится, а битая
// ссылка на странице с юридическим текстом — худшее место для битой ссылки.
//
// Сайт рассказывает, что продукт не делает сетевых вызовов. Страница, которая
// при этом сама тянет шрифт из Google или счётчик из чужого домена, обесценивает
// это утверждение целиком — и такой регресс добавляется одной строкой в шаблоне,
// незаметно для ревью. Поэтому проверка машинная.
//
// Считаются ТОЛЬКО подгружаемые ресурсы: <script src>, <link href>, <img src>,
// <iframe src>, srcset, url() в инлайн-стилях. Обычные ссылки <a href> на
// github.com — это навигация по клику пользователя, а не запрос браузера.
//
// Использование: node scripts/check-site-assets.mjs <dist-dir>

import { existsSync, statSync } from 'node:fs';
import { readFile, readdir } from 'node:fs/promises';
import { join, relative } from 'node:path';

const dist = process.argv[2];
if (!dist) {
  console.error('usage: node scripts/check-site-assets.mjs <dist-dir>');
  process.exit(2);
}

// Совпадает с base из astro.config.mjs. Резолвится через URL, поэтому origin
// фиктивный — важен только путь.
const BASE = process.env.SITE_BASE ?? '/wotold';
const ORIGIN = 'http://site.invalid';

const ANCHOR = /<a\b[^>]*?\bhref\s*=\s*["']([^"']+)["']/gi;

/** Страница существует, если под её путём лежит index.html или сам файл. */
function pageExists(pathname) {
  const rel = pathname.slice(BASE.length).replace(/^\/+/, '').replace(/\/+$/, '');
  const candidates = [join(dist, rel, 'index.html'), join(dist, rel), join(dist, `${rel}.html`)];
  return candidates.some((c) => existsSync(c) && statSync(c).isFile());
}

// Подгружающие атрибуты. <a href> сюда намеренно не входит.
const SUBRESOURCE = [
  /<(?:script|iframe|img|source|video|audio|embed|track)\b[^>]*?\bsrc\s*=\s*["']([^"']+)["']/gi,
  /<[^>]*?\bsrcset\s*=\s*["']([^"']+)["']/gi,
  /<[^>]*?\bstyle\s*=\s*["'][^"']*url\(\s*['"]?([^'")]+)/gi,
  /@import\s+(?:url\(\s*)?['"]([^'"]+)['"]/gi,
];

// <link> разбирается отдельно: тег двойного назначения. rel=stylesheet или
// preload — это запрос браузера, а rel=canonical / alternate / hreflang —
// метаданные, и там абсолютный URL собственного сайта нормален и обязателен.
// Без этого различия гейт валился на каждой странице из-за своей же canonical.
const LINK_TAG = /<link\b[^>]*>/gi;
const FETCHING_REL = new Set([
  'stylesheet',
  'preload',
  'modulepreload',
  'prefetch',
  'preconnect',
  'dns-prefetch',
  'icon',
  'shortcut',
  'apple-touch-icon',
  'apple-touch-startup-image',
  'mask-icon',
  'manifest',
]);

/** URL-ы, которые запросит браузер, из всех <link> страницы. */
function linkSubresources(html) {
  const urls = [];
  LINK_TAG.lastIndex = 0;
  let tag;
  while ((tag = LINK_TAG.exec(html))) {
    const rel = /\brel\s*=\s*["']([^"']+)["']/i.exec(tag[0])?.[1] ?? '';
    const href = /\bhref\s*=\s*["']([^"']+)["']/i.exec(tag[0])?.[1];
    if (!href) continue;
    const fetches = rel
      .toLowerCase()
      .split(/\s+/)
      .some((token) => FETCHING_REL.has(token));
    if (fetches) urls.push(href);
  }
  return urls;
}

/** Ресурс внешний, если у него есть схема или он протокол-относительный. */
const isExternal = (url) => /^(?:[a-z][a-z0-9+.-]*:)?\/\//i.test(url) && !/^data:/i.test(url);

async function* walk(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(full);
    else if (entry.name.endsWith('.html')) yield full;
  }
}

const findings = [];
const brokenLinks = [];
let scanned = 0;

for await (const file of walk(dist)) {
  scanned += 1;
  const html = await readFile(file, 'utf8');
  const here = relative(dist, file);
  // URL страницы: dist/legal/terms/index.html → <base>/legal/terms/
  const pageUrl = new URL(
    `${BASE}/${here.replace(/index\.html$/, '').split('\\').join('/')}`,
    ORIGIN,
  );

  ANCHOR.lastIndex = 0;
  let a;
  while ((a = ANCHOR.exec(html))) {
    const href = a[1];
    if (/^(?:[a-z][a-z0-9+.-]*:|#)/i.test(href)) continue;
    const { pathname } = new URL(href, pageUrl);
    if (!pathname.startsWith(`${BASE}/`) && pathname !== BASE) {
      brokenLinks.push({ file: here, href, resolved: pathname, why: `вне base ${BASE}` });
    } else if (!pageExists(pathname)) {
      brokenLinks.push({ file: here, href, resolved: pathname, why: 'страницы нет' });
    }
  }

  const candidates = [...linkSubresources(html)];

  for (const re of SUBRESOURCE) {
    re.lastIndex = 0;
    let m;
    while ((m = re.exec(html))) {
      // srcset — список «url размер, url размер».
      candidates.push(...m[1].split(',').map((part) => part.trim().split(/\s+/)[0]));
    }
  }

  for (const url of candidates) {
    if (url && isExternal(url)) {
      findings.push({ file: relative(process.cwd(), file), url });
    }
  }
}

if (scanned === 0) {
  console.error(`[check-site-assets] В ${dist} не найдено ни одного .html — сборка пустая?`);
  process.exit(1);
}

let failed = false;

if (findings.length > 0) {
  failed = true;
  const seen = new Set();
  const lines = [];
  for (const f of findings) {
    const key = `${f.file} ${f.url}`;
    if (seen.has(key)) continue;
    seen.add(key);
    lines.push(`  ${f.file}\n    ${f.url}`);
  }
  console.error(`[check-site-assets] Сторонние ресурсы (${seen.size}):`);
  console.error(lines.join('\n'));
  console.error(
    '[check-site-assets] Сайт обещает ноль запросов на чужие хосты. ' +
      'Инлайнь ресурс или клади его в apps/site/public/.\n',
  );
}

if (brokenLinks.length > 0) {
  failed = true;
  const seen = new Map();
  for (const b of brokenLinks) seen.set(`${b.file} ${b.href}`, b);
  console.error(`[check-site-assets] Битые внутренние ссылки (${seen.size}):`);
  for (const b of seen.values()) {
    console.error(`  ${b.file}\n    ${b.href} → ${b.resolved}  [${b.why}]`);
  }
  console.error(
    '[check-site-assets] Относительные ссылки резолвятся от URL страницы: ' +
      'со страницы /legal/terms/ до /consent/ путь ../../consent/, а не ../consent/.\n',
  );
}

if (failed) process.exit(1);

console.log(`[check-site-assets] ${scanned} страниц: сторонних ресурсов нет, ссылки целы.`);
