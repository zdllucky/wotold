#!/usr/bin/env node
// PostToolUse hook для Write/Edit.
// [B17] Wotold Atelier v2 design gate enforcement:
//
//   - На .tsx / .ts / .css правках предупреждает (НЕ блокирует) если в diff
//     встречаются:
//       * сырые hex-цвета (#RRGGBB, #RGB) — должны быть var(--*) из tokens.css
//       * сырые oklch() значения — то же
//       * legacy --color-* токены — должны быть мигрированы на новый набор
//         (var(--bg), var(--ink), var(--accent), ...)
//
//   - Whitelist: handoff source files (tokens.css, wotold.css, legacy-tokens.css,
//     docs/design/atelier-v2/**) — там сырые значения легитимны.
//
// Не блокируем — предупреждение в stderr. Гейт «жёсткий» уровень — на code-review.
//
// Источник: docs/design/atelier-v2/README.md, .claude/skills/design-gate/SKILL.md.

let data = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk) => (data += chunk));
process.stdin.on('end', () => {
  let input;
  try {
    input = JSON.parse(data);
  } catch {
    process.exit(0);
  }

  const path = (input?.tool_input?.file_path ?? '').toString();
  const content = (
    input?.tool_input?.content ??
    input?.tool_input?.new_string ??
    ''
  ).toString();

  if (!path || !content) process.exit(0);

  // Только UI surface.
  if (!/\.(tsx?|css)$/.test(path)) process.exit(0);

  // Whitelist — handoff source + token files.
  const WHITELIST = [
    /\/styles\/tokens\.css$/,
    /\/styles\/wotold\.css$/,
    /\/styles\/legacy-tokens\.css$/,
    /\/styles\/fonts\.css$/,
    /\/docs\/design\/atelier-v2\//,
    /\.claude\//,
  ];
  if (WHITELIST.some((re) => re.test(path))) process.exit(0);

  const findings = [];

  // Сырые hex в content.
  const hexRe = /#[0-9a-fA-F]{3,8}\b/g;
  const hexMatches = content.match(hexRe) ?? [];
  if (hexMatches.length > 0) {
    findings.push(
      `[design-gate] Сырой hex: ${[...new Set(hexMatches)].slice(0, 5).join(', ')}\n` +
        `[design-gate] → используй var(--*) из tokens.css (--ink/--accent/--signal/--bg/...)`,
    );
  }

  // Сырые oklch в content.
  const oklchRe = /oklch\s*\(/g;
  if (oklchRe.test(content)) {
    findings.push(
      `[design-gate] Сырой oklch() — только в tokens.css. ` +
        `Используй var(--*) из tokens.css.`,
    );
  }

  // Legacy --color-* токены.
  const legacyRe = /--color-(bg|surface|text|border|accent|danger|success|warning)[a-z-]*/g;
  const legacyMatches = content.match(legacyRe) ?? [];
  if (legacyMatches.length > 0) {
    findings.push(
      `[design-gate] Legacy токены: ${[...new Set(legacyMatches)].slice(0, 5).join(', ')}\n` +
        `[design-gate] → мигрируй на новый набор (--bg, --ink, --accent, --signal, --line, --muted и т.д.)\n` +
        `[design-gate] см. docs/design/atelier-v2/tokens.css + legacy-tokens.css mapping.`,
    );
  }

  if (findings.length > 0) {
    console.error('[design-gate] ⚠ Atelier v2 design gate warnings for:', path);
    for (const f of findings) console.error(f);
    console.error('[design-gate] Подробнее: .claude/skills/design-gate/SKILL.md');
  }

  process.exit(0);
});
