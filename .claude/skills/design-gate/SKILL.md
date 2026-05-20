---
name: design-gate
description: MANDATORY pre-implementation gate for any UI/CSS/React-component work in Wotold. Validates that the proposed change references the Atelier v2 handoff (docs/design/atelier-v2) and does not introduce tokens, layout primitives, or visual patterns outside the design system. Trigger BEFORE planning or writing any .tsx/.css/.module.css edit that affects visual surface.
origin: wotold
---

# Wotold Design Gate (Atelier v2)

This is a **mandatory validation step** before any UI work. Skipping it is a workflow violation per `CLAUDE.md`.

## When to invoke

BEFORE you plan or implement any of:

- New React component or page rewrite
- Any `.tsx` change touching className/JSX structure beyond a single attribute
- Any `.css` edit (including pages.css, ui.css, *.module.css, inline styles)
- Any introduction of new color/spacing/typography value
- Any modal, toast, dialog, navigation, list, card, button variant
- Any layout primitive (shell, rail, grid)

## The gate (must answer ALL before writing)

1. **Reference source.** Which file in `docs/design/atelier-v2/` covers this surface?
   - Foundation tokens → `tokens.css`
   - Component class → `wotold.css`
   - Page mapping → `MIGRATION.md` (sections 1–10)
   - Theme/accent → `useTheme.tsx`
   - Visual reference → `_reference/atelier-2.jsx`, `_reference/atelier-styles.css`
2. **Tokens only.** Will every color/spacing/radius/shadow come from `var(--*)` defined in `tokens.css`? If you need a value that isn't a token — STOP and propose a new token first.
3. **Component class first.** Is there an existing `wotold.css` class (`.btn`, `.card`, `.tabs`, `.transcript-row`, `.field`, `.input`, `.sp`, `.rec-btn`, `.stat`, `.nav-item`, `.app-rail`, `.app-shell`, `.app-main`, `.tab`, `.modal-backdrop`, `.index-card`, `.dot`, `.conf`, `.empty`, `.divider`, `.wave-lane`)? If yes, use it — don't recreate.
4. **Two-color rule.** Bordeaux `var(--accent)` is for everything UI. `var(--signal)` (red) is reserved for **recording state + destructive actions only**. No exceptions.
5. **Theme + accent orthogonal.** Anything you render must read correctly across `data-theme=light|dark` × `data-accent=bordeaux|persian|ink`. Never inline a single-mode hex.
6. **Type pairing.** Hero/display/title/subtitle/transcript-text → Source Serif 4 (`var(--font-serif)`). UI labels/buttons/nav → DM Sans (`var(--font-sans)`). Timestamps/IDs/codes → JetBrains Mono (`var(--font-mono)`). No defaults.
7. **Logic preserved.** For migrations — does the new JSX preserve every existing `useEffect`, `useState`, API call, event listener, hotkey, consent gate, updater check? Per `R-series` constraints in `docs/ПАСПОРТ_ПРОЕКТА.md` § 12.
8. **Accessibility floor.** Interactive elements ≥24×24 CSS px. Modals trap focus + ESC. Color is never the only signal. Reduced-motion respected.

## Outputs to produce BEFORE coding

A short alignment block in the chat:

```text
[design-gate] Surface: <page or component name>
Reference: docs/design/atelier-v2/<file>:<section or line range>
Tokens used: <list>
Classes used: <list>
New tokens needed: <none | list with justification>
Logic preserved: <yes — list of preserved useEffects/handlers>
A11y: <focus, target size, ARIA notes>
```

If any answer is "I don't know" — STOP. Read the relevant handoff file first.

## Related skills (run alongside)

- `frontend-design-direction` — direction sanity check (avoid template/AI-slop fallbacks).
- `design-system` — token consistency audit.
- `accessibility` — WCAG 2.2 AA spec for the surface.
- `motion-ui` — motion patterns when introducing transitions.
- `frontend-patterns` — composition/state patterns.

## Related agents

- `a11y-architect` — invoke for any modal/dialog/form/navigation surface.
- `code-reviewer` / `typescript-reviewer` — mandatory after the diff.

## Anti-patterns (auto-fail this gate)

- Raw hex (`#RRGGBB`) or `oklch()` in `.tsx` or any `.css` other than `tokens.css` / handoff sources.
- Reintroducing `--color-*` from the old token set in new code (use `--bg`, `--ink`, `--accent`, etc. from the Atelier v2 set).
- Using `--signal` (red) for hover, active, links, or accent.
- Adding new font stacks.
- Inline magic spacing (`marginTop: 23`) — use `var(--space-N)` or stick to spacing already in handoff samples.
- Skipping `<ThemeProvider>` for new screens.
- "Looks fine on light theme" — must verify dark + 3 accents.

## How this gate is enforced

1. PreToolUse hook `scripts/hooks/design-gate.mjs` (added in this rollout) warns on `.tsx`/`.css` writes that contain raw hex, raw oklch, or known old-token names outside the handoff sources.
2. CLAUDE.md § "Design gate" lists this as a required step in the PDCA loop.
3. PR review (`/code-review`) must explicitly call out the alignment block above; missing it → request changes.
