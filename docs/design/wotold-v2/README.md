# Wotold v2 — дизайн (поколение поверх Atelier v2)

> Канонический источник UI с B18. При расхождении с Atelier v2 (`../atelier-v2/`) —
> **побеждает Wotold v2**. Atelier v2 — legacy, остаётся для истории до конца миграции.
> Паспорт (W6) по-прежнему выше дизайна при конфликте с R1–R13.

## Источник истины

Своего «нарисованного» handoff-дока у Wotold v2 нет — спека = **код прототипа**:

- `~/Downloads/Wotold v2/` — браузерный прототип:
  - `uikit.css` — токены + component-классы (§1 tokens → перенесён в `apps/desktop/src/styles/tokens.css`; §2–5 base/layout/components → `styles/wk.css`).
  - `uikit.jsx` / `uikit-icons.jsx` — примитивы + line-icon set (порт → `src/ui/Icon.tsx`).
  - `wk-*.jsx` — экраны (shell, Inbox, call-detail, contacts, settings, DS).
  - входная точка `Wotold v2.html`.
- Инвентарь/анализ пакета: `scratchpad/v2-analysis.md` (регенерится workflow `wotold-v2-redesign-analysis`).
- План и итерации: `docs/ROADMAP.md` §«Wotold v2 Redesign (B18)».

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

Atelier-имена (`--ink --line --signal --space-* --font-serif …`) держатся через
`styles/legacy-tokens.css` shim до B18.6 — в **новом** коде использовать uikit-набор.

## Статус миграции

См. чек-бокс-список `docs/ROADMAP.md` §B18 (B18.0 foundation → B18.6 cleanup/QA).
