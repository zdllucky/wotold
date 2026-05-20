---
description: Run the Wotold Atelier v2 design gate — mandatory before any UI/CSS work.
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

- `docs/design/atelier-v2/README.md`
- `docs/design/atelier-v2/MIGRATION.md`
- `docs/design/atelier-v2/tokens.css`
- `docs/design/atelier-v2/wotold.css`

If a surface is not yet mapped in `MIGRATION.md`, STOP and update `MIGRATION.md` first.
