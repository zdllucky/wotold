---
description: Run the Wotold v2 (uikit) design gate — mandatory before any UI/CSS work.
---

# /design-gate

Invokes the `design-gate` skill (project-local, `.claude/skills/design-gate/SKILL.md`).

## When

Before any of:

- New React component or page rewrite
- Any `.tsx` change touching className/JSX structure beyond a single attribute
- Any `.css` or inline-style edit
- Any new color/spacing/typography value

## Usage

```text
/design-gate <surface name>
```

The skill output is a structured alignment block. No code is written by this command — only the gate.

## Source of truth

- `docs/design/wotold-v2/README.md` — canon summary
- `docs/design/wotold-v2/_reference/` — the prototype itself (spec = code; open `index.html`)
- `docs/design/wotold-v2/assistant.md` + `_reference-assistant/` — assistant surfaces (M15/B24)
- `apps/desktop/src/styles/{tokens,wk,components}.css` — the live implementation

`docs/design/atelier-v2/` is the superseded generation — history only, do not use it as a reference.
