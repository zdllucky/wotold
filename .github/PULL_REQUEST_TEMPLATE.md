<!--
PR title must follow Conventional Commits — commitlint checks it in CI.
  feat(assistant): hybrid retrieval via RRF
  fix(pipeline): keep final chunk on halt
A milestone-style title ("M15: ...") fails the run.
-->

## What and why

<!-- What changes, and what problem it solves. Link the ROADMAP item if there is one. -->

Closes #

## How it was verified

<!-- Commands you actually ran, and what you checked by hand. Not a wish list. -->

- [ ] `cargo fmt --check` · `cargo clippy -- -D warnings` · `cargo test`
- [ ] `pnpm -r typecheck` · `pnpm -r test`
- [ ] Ran the real app (`pnpm tauri dev`) for anything user-facing

## Tests

- [ ] New behaviour is covered by tests
- [ ] For a bug fix: **the regression test was confirmed to fail before the fix** (verified by reverting it)
- [ ] For orchestration or glue code: happy path **and** at least one failure path
- [ ] Coverage thresholds unchanged — lowering them needs explicit maintainer agreement

## UI changes

<!-- Delete this whole section if the PR touches no .tsx / .css / inline styles. -->

Design gate is mandatory before any visual change — see
[`docs/design/wotold-v2/README.md`](https://github.com/zdllucky/wotold/blob/main/docs/design/wotold-v2/README.md).

```text
[design-gate] Surface:
Reference: docs/design/wotold-v2/_reference/
Tokens used:
Classes used:
New tokens needed: none
Logic preserved: yes —
A11y:
```

- [ ] Checked in **light and dark**
- [ ] No raw hex/oklch — everything through `var(--*)` from `tokens.css`
- [ ] `var(--danger)` used only for recording and destructive actions
- [ ] Every new user-visible string goes through `t()` in **all three** locales (ru/en/kk)
- [ ] Logic preserved 1:1 — hotkeys, consent gates, effects, API calls

## Checks specific to this repo

- [ ] Twin parity considered — if this fixes a bug in one of a paired module (`AudioRecorder` ↔ `ProcessTapRecorder`, chunk FSM ↔ call lifecycle, `searchCalls` ↔ `findContactsByName`), the twin was checked for the same hole
- [ ] Any warn-and-continue path that affects a call's result sets a visible degraded flag — log-only is not acceptable
- [ ] CPU work over ~10 ms runs inside `spawn_blocking`
- [ ] No `sleep()` used to synchronize tests with background work
- [ ] Ids from the webview or MCP are UUID-validated before touching a path, plus `ensure_path_under`
- [ ] No file grew past 800 lines (measured on the resulting file)

## Security

- [ ] This PR touches none of the security-sensitive modules listed in [`CONTRIBUTING.md`](https://github.com/zdllucky/wotold/blob/main/CONTRIBUTING.md#security-sensitive-areas)
- [ ] It does — a maintainer security review is requested, and the threats considered are described above

## Notes for the reviewer

<!-- Trade-offs, things you are unsure about, follow-ups you deliberately left out. -->
