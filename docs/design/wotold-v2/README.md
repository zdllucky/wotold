# Wotold v2 — дизайн (поколение поверх Atelier v2)

> Канонический источник UI с B18. При расхождении с Atelier v2 (`../atelier-v2/`) —
> **побеждает Wotold v2**. Atelier v2 — legacy, остаётся для истории до конца миграции.
> Паспорт (W6) по-прежнему выше дизайна при конфликте с R1–R13.

## Источник истины

Своего «нарисованного» handoff-дока у Wotold v2 нет — спека = **код прототипа**:

- [`_reference/`](_reference/) — браузерный прототип, завендорен в репо ([TD-04]; открывается
  `index.html` прямо из файловой системы, React/Babel тянутся с CDN):
  - `uikit.css` — токены + component-классы (§1 tokens → перенесён в `apps/desktop/src/styles/tokens.css`; §2–5 base/layout/components → `styles/wk.css`).
  - `uikit.jsx` / `uikit-icons.jsx` — примитивы + line-icon set (порт → `src/ui/Icon.tsx`).
  - `wk-*.jsx` — экраны (shell, Inbox, call-detail, contacts, settings, DS).
  - `wk-views.jsx` / `wk-explore.jsx` — виды и поиск инбокса; не грузятся точкой входа,
    но это исходники для открытых пунктов B18 (Views + Explore).
- [`_reference-assistant/`](_reference-assistant/) — хендофф раздела «Ассистент» (M15/B24),
  отдельная точка входа + `01-SPEC.md`. Канон-аддендум: [`assistant.md`](assistant.md).
- Оба снапшота **заморожены**: это точка во времени, правки идут в код, не в них.
- План и итерации: [`ROADMAP_ARCHIVE.md`](../../ROADMAP_ARCHIVE.md) §B18.

## Зафиксированные решения (B18)

- **Шрифты**: Hanken Grotesk + IBM Plex Mono (serif выпилен).
- **Акцент**: моно-графит (`ink`), один набор, без picker. Тема: light + dark.
- **Density**: фикс `cozy` (`<html data-density="cozy">`).
- **Home** удалён, default-экран = Inbox; запись = dock + floating widget; ⌘K palette.
- **Assistant**-таб отложен отдельной доработкой.

## Token / class канон (uikit)

- Поверхности: `--bg --sunken --panel --raised --hover --active`.
- Текст: `--text --text-2 --text-3 --text-faint`. Бордеры: `--border --border-2 --border-strong`.
- Акцент: `--accent --accent-hover --accent-press --accent-soft --accent-line --accent-text --on-accent`.
- Семантика: `--danger* --ok* --warn* --info-soft`. Speaker: `--sp1..5`.
- Шкалы: `--t-11..28`, `--s1..9`, `--r-xs..pill`, `--fast/base/slow + --ease`.
- `--signal` (красный) старого набора → `--danger`; **только запись и деструктив**.

Atelier-имена (`--ink --line --signal --space-* --font-serif …`) **удалены** вместе с
shim'ом `legacy-tokens.css` в B18.6. Встретил такой токен — это мёртвый код, а не легаси-мост.

## Слои CSS

1. [`styles/tokens.css`](../../../apps/desktop/src/styles/tokens.css) — токены (§1 uikit), light + dark.
2. [`styles/wk.css`](../../../apps/desktop/src/styles/wk.css) — примитивы uikit (§2–5): `.btn`, `.iconbtn`, `.input`, `.tabs`, `.trow`, `.turn`, …
3. [`styles/components.css`](../../../apps/desktop/src/styles/components.css) — app-специфичные классы (transcript / pipeline / rec-float / banners / modal-frame и пр.), порт Atelier, token-clean.

React-обёртки над классами — `src/ui/*`; иконки — `src/ui/Icon.tsx`.

## Статус миграции

Миграция Atelier→v2 **завершена** (B18.0 foundation → B18.6 cleanup, shim удалён).
Последующие батчи полиша (B20–B30) — в [`ROADMAP_ARCHIVE.md`](../../ROADMAP_ARCHIVE.md);
живые остатки — [`ROADMAP.md`](../../ROADMAP.md) §«B18 · остатки».
