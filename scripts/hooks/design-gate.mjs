#!/usr/bin/env node
// PostToolUse hook для Write/Edit.
// [B18.6] Wotold v2 (uikit) design gate enforcement:
//
//   - На .tsx / .ts / .css правках предупреждает (НЕ блокирует) если в diff
//     встречаются:
//       * сырые hex-цвета (#RRGGBB, #RGB) — должны быть var(--*) из tokens.css
//       * сырые oklch() значения — то же
//       * legacy --color-* токены — должны быть мигрированы на новый набор
//         (var(--bg), var(--text), var(--accent), ...)
//
//   - Whitelist: token/component source files (tokens.css, wk.css,
//     components.css, fonts.css, docs/design/wotold-v2/**) — там сырые
//     значения легитимны.
//
// Не блокируем — предупреждение в stderr. Гейт «жёсткий» уровень — на code-review.
//
// Источник: docs/design/wotold-v2/README.md, .claude/skills/design-gate/SKILL.md.

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

  // [TD-03] Whitelist проверяется по пути ОТНОСИТЕЛЬНО корня репо, а не по
  // абсолютному. Иначе правило `.claude/` матчило сам путь git-worktree
  // (<repo>/.claude/worktrees/<name>/…), и внутри worktree гейт молча
  // пропускал АБСОЛЮТНО ВСЁ — а фича-работа тут и ведётся.
  const root = process.env.CLAUDE_PROJECT_DIR ?? '';
  const rel = root && path.startsWith(root) ? path.slice(root.length) : path;

  // Whitelist — token/component sources + вендоренный прототип + харнесс.
  const WHITELIST = [
    /\/styles\/tokens\.css$/,
    /\/styles\/wk\.css$/,
    /\/styles\/components\.css$/,
    /\/styles\/fonts\.css$/,
    /^\/?docs\/design\//,
    /^\/?\.claude\//,
    // Единственный стилевой файл сайта. Он импортирует канон приложения и
    // раскладывает переменные Starlight на токены Wotold; сырых цветов в нём
    // нет и быть не должно, но маркетинговые токены типографики и ритма
    // (--t-hero, --s-section) живут именно там, а не в tokens.css —
    // приложению они не нужны.
    /^\/?apps\/site\/src\/styles\/site\.css$/,
  ];
  if (WHITELIST.some((re) => re.test(rel))) process.exit(0);

  const findings = [];

  // Сырые hex в content.
  const hexRe = /#[0-9a-fA-F]{3,8}\b/g;
  const hexMatches = content.match(hexRe) ?? [];
  if (hexMatches.length > 0) {
    findings.push(
      `[design-gate] Сырой hex: ${[...new Set(hexMatches)].slice(0, 5).join(', ')}\n` +
        `[design-gate] → используй var(--*) из tokens.css (--text/--accent/--danger/--bg/--border/...)`,
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
        `[design-gate] → канон Wotold v2 (uikit): --bg, --text, --accent, --danger, --border, --text-3 и т.д.\n` +
        `[design-gate] Atelier-имена (--ink/--line/--signal/...) удалены в B18.6 — мигрируй на uikit-токены.\n` +
        `[design-gate] см. docs/design/wotold-v2/ + styles/tokens.css.`,
    );
  }

  if (findings.length > 0) {
    console.error('[design-gate] ⚠ Wotold v2 (uikit) design gate warnings for:', path);
    for (const f of findings) console.error(f);
    console.error('[design-gate] Подробнее: .claude/skills/design-gate/SKILL.md');
  }

  process.exit(0);
});
