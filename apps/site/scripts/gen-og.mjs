/**
 * Генерация статичной OG-картинки (public/og.png, 1200×630) из марки Wotold
 * (SPEC §8 handoff 2026-07-30). Запускается разово, артефакт коммитится —
 * в сборке скрипт не участвует, внешних хостов в рантайме сайта нет.
 *
 * Wordmark набран Onest ExtraBold. Шрифт в системе обычно не установлен,
 * поэтому текст рендерится через fontconfig: положи Onest-ExtraBold.ttf в
 * каталог и укажи его в FONTCONFIG_FILE (см. apps/desktop/public/fonts/README.md
 * про источник шрифтов):
 *
 *   FONTCONFIG_FILE=/path/to/fonts.conf node scripts/gen-og.mjs
 *
 * Hex захардкожены намеренно: ассет живёт вне страницы и токенов не видит;
 * значения — светлая тема tokens.css (--bg / --text / --text-faint).
 */
import sharp from 'sharp';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const out = path.join(path.dirname(fileURLToPath(import.meta.url)), '../public/og.png');

const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630">
  <rect width="1200" height="630" fill="#FAFAFB"/>
  <!-- марка: viewBox 72×50, масштаб ×3.6 → 259×180 -->
  <g transform="translate(255 225) scale(3.6)">
    <rect x="3" y="6" width="56" height="17" rx="8.5" fill="#1A1B23"/>
    <rect x="13" y="27" width="56" height="17" rx="8.5" fill="#9C9FAB"/>
  </g>
  <text x="555" y="361" font-family="Onest" font-weight="800" font-size="118"
    letter-spacing="-3" fill="#1A1B23">Wotold</text>
</svg>`;

await sharp(Buffer.from(svg), { density: 144 }).png().toFile(out);
console.log(`og.png written: ${out}`);
