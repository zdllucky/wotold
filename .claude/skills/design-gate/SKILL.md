---
name: design-gate
description: MANDATORY pre-implementation gate for any UI/CSS/React-component work in Wotold. Validates that the proposed change references the Wotold v2 (uikit) canon (docs/design/wotold-v2) and does not introduce tokens, layout primitives, or visual patterns outside the design system. Trigger BEFORE planning or writing any .tsx/.css/.module.css edit that affects visual surface.
origin: wotold
---

# Wotold Design Gate (Wotold v2 / uikit)

**Mandatory validation step** before any UI work. Skipping it is a workflow violation per `CLAUDE.md`.

> Canon is **Wotold v2 (uikit)**. The Atelier v2 generation was superseded in B18 and its
> shim removed in B18.6 — `docs/design/atelier-v2/` is kept for history only. If you find
> guidance naming Bordeaux accents, Source Serif, `--signal` or `--ink`, it is Atelier-era
> and wrong.

## When to invoke

BEFORE you plan or implement any of:

- New React component or page rewrite
- Any `.tsx` change touching className/JSX structure beyond a single attribute
- Any `.css` edit (including `components.css`, `ui.css`, `*.module.css`, inline styles)
- Any introduction of new color/spacing/typography value
- Any modal, toast, dialog, navigation, list, card, button variant
- Any layout primitive (shell, rail, grid)

## The gate (must answer ALL before writing)

1. **Reference source.** Which file covers this surface?
   - Prototype (spec = code) → `docs/design/wotold-v2/_reference/` (`uikit.css`, `wk-*.jsx`; open `index.html`)
   - Assistant surfaces (M15/B24) → `docs/design/wotold-v2/_reference-assistant/` + `assistant.md`
   - Canon summary → `docs/design/wotold-v2/README.md`
   - Live tokens → `apps/desktop/src/styles/tokens.css`
   - Live primitives → `apps/desktop/src/styles/wk.css`
   - Live app-classes → `apps/desktop/src/styles/components.css`
2. **Tokens only.** Every color/spacing/radius/shadow comes from `var(--*)` in `tokens.css`. Need a value that isn't a token — STOP and propose the token first.
3. **Component class first.** Use the existing class before inventing markup: `.btn` (+`--primary/--default/--ghost/--soft/--danger`), `.iconbtn`, `.chip`, `.avatar`, `.dot`, `.kbd`, `.input`, `.field`, `.seg`, `.switch`, `.tabs`/`.tab`, `.navitem`, `.rail`/`.minirail`, `.menu`, `.overlay`/`.modal`, `.palette`, `.tbl`/`.trow`, `.turn`, `.doc`, `.rrail`, `.composer-dock`, `.rec-widget`, `.wave`, `.optioncard`, `.setting-row`. App-specific ones live in `components.css`.
4. **One accent.** `var(--accent)` is mono-graphite and covers everything UI. `var(--danger)` (red) is reserved for **recording state + destructive actions only**. No exceptions. The accent picker was removed in B18.5 — there is exactly one accent.
5. **Both themes.** Anything you render must read correctly across `data-theme=light|dark`. Density is fixed `cozy`. Never inline a single-mode hex.
6. **Type pairing.** UI / body / transcript → **Hanken Grotesk**. Timestamps / IDs / code → **IBM Plex Mono**. Serif was removed in B18 — do not reintroduce a third family. Sizes come from the `--t-11..28` scale.
7. **Logic preserved.** For migrations — does the new JSX preserve every existing `useEffect`, `useState`, API call, event listener, hotkey, consent gate, updater check? Per `R-series` constraints in `docs/ПАСПОРТ_ПРОЕКТА.md` § 12.
8. **Accessibility floor.** Interactive targets ≥24×24 CSS px (extend the hitbox with an `::after` inset if the visual is smaller). Modals trap focus + ESC. Visible `:focus-visible` ring. Color is never the only signal. Reduced-motion respected.

## Outputs to produce BEFORE coding

A short alignment block in the chat:

```text
[design-gate] Surface: <page or component name>
Reference: docs/design/wotold-v2/_reference/<file>|uikit.css
Tokens used: <list>
Classes used: <list>
New tokens needed: <none | list with justification>
Logic preserved: <yes — list of preserved useEffects/handlers>
A11y: <focus, target size, ARIA notes>
```

If any answer is "I don't know" — STOP. Read the reference first.

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

- Raw hex (`#RRGGBB`) or `oklch()` in `.tsx` or any `.css` other than `tokens.css` / vendored reference.
- Reintroducing deleted Atelier tokens (`--ink`, `--line`, `--signal`, `--space-*`, `--font-serif`) or legacy `--color-*` names.
- Using `var(--danger)` for hover, active, links, or accent.
- Adding new font stacks.
- Inline magic spacing (`marginTop: 23`) — use `var(--s1..9)`; inline `fontSize` — use `var(--t-11..28)`.
- "Looks fine on light theme" — must verify dark too.

## How this gate is enforced

1. PostToolUse hook `scripts/hooks/design-gate.mjs` warns on `.tsx`/`.css` writes containing raw hex, raw oklch, or legacy `--color-*` outside whitelisted sources (tokens/wk/components/fonts CSS, `docs/design/**`, `.claude/**`). The whitelist is matched on the repo-relative path — an absolute match used to treat every file in a git-worktree as whitelisted.
2. `CLAUDE.md` § "Design Gate" lists this as a required step in the PDCA loop.
3. PR review (`/code-review`) must explicitly call out the alignment block above; missing it → request changes.
