# Contributing to Wotold

Thanks for considering a contribution. This document is the short version of how
this repository works. It is deliberately specific — Wotold enforces several
rules mechanically, and a PR that ignores them will fail CI or review.

## Before you start

**Read section 12 of the spec**, [`docs/ПАСПОРТ_ПРОЕКТА.md`](docs/ПАСПОРТ_ПРОЕКТА.md).
It lists deliberate limitations (R2, R3, R4, R6, R9–R13) that are **not bugs and
must not be "fixed"**. Examples:

| Marker | Accepted limitation |
|---|---|
| R2 | LLM speaker guessing is a booster only — never auto-assigns a contact |
| R3 | No automatic "a call is happening" detection; manual button only |
| R4 | Windows capture is `unimplemented!()` behind the `AudioCapture` trait |
| R6 | macOS builds are not Apple-notarized in the MVP; Gatekeeper "Open anyway" is expected |
| R9 | The local engine is macOS-only in the MVP |
| R10 | Models are downloaded on demand, never bundled into the installer |
| R11 | Local STT is offline-only; live realtime captions are out of scope |
| R12 | Local LLM summaries are lower quality than cloud; the UI says so explicitly |
| R13 | Weak hardware does not block the local engine — the probe recommends Light with a warning |

If you disagree with one of these, open a
[Discussion](https://github.com/zdllucky/wotold/discussions) rather than a PR.
When the spec and any other document disagree, the spec wins.

Most internal documentation is in Russian — that is the project's working
language. Issues and pull requests in English are welcome and expected.

## Setup

Requirements:

- **macOS 14.2+** — Core Audio process tap and the local engine are macOS-only (R9). Linux and Windows have trait stubs that return `unimplemented!()`.
- **Node.js ≥ 20.18** (`.nvmrc` pins 20.18.1)
- **pnpm ≥ 10**
- **Rust** stable + Tauri CLI 2 — `cargo install tauri-cli --version "^2"`

```bash
git clone https://github.com/zdllucky/wotold.git
cd wotold
pnpm install
pnpm tauri dev
```

`scripts/dev.sh` clears stale processes and port 5173 before starting, and can
auto-restart on Rust changes if `watchexec` or `entr` is installed.

## Workflow

The repository follows a plan → implement → verify → review loop. For any
non-trivial change:

### 1. Design gate — mandatory for any UI change

Before editing any `.tsx`, `.css`, `.module.css` or inline style:

1. Read [`docs/design/wotold-v2/README.md`](docs/design/wotold-v2/README.md) and
   compare the screen against the frozen prototype in
   [`docs/design/wotold-v2/_reference/`](docs/design/wotold-v2/_reference/).
2. Include an alignment block in the PR description:

   ```text
   [design-gate] Surface: <page/component>
   Reference: docs/design/wotold-v2/_reference/<wk-file>|uikit.css
   Tokens used: <list>
   Classes used: <list>
   New tokens needed: <none | list>
   Logic preserved: <yes — list>
   A11y: <focus, target, ARIA>
   ```

Hard rules:

- **No raw colors.** Every color, spacing, radius and shadow goes through
  `var(--*)` from [`apps/desktop/src/styles/tokens.css`](apps/desktop/src/styles/tokens.css).
  Raw hex or `oklch()` outside the whitelisted token sources is a review blocker.
  A PostToolUse hook warns about it locally.
- **Use the existing component classes** from `wk.css` and `components.css`, and
  the React wrappers in `src/ui/*`. Icons come from `<Icon name=… />` — no emoji.
- **`var(--danger)` (red) is reserved** for recording state and destructive
  actions. Every other accent is `var(--accent)`.
- **Light and dark must both work.** There is one accent (graphite `ink`) and
  density is fixed at `cozy`, so that is two combinations to check, not six.
- **Logic is preserved 1:1** — hotkeys, consent gates, effects and API calls do
  not change during a visual migration.

### 2. Implement, tests first where it counts

- **Algorithmic modules** (matching, parsing, utilities, DB repositories,
  middleware) are test-first. Write the failing test, then the implementation.
- **Glue is tested first, always.** Orchestration — `pipeline::run`, recovery
  flows, hook composition — needs a happy-path test plus at least one fail-path
  test before it can be marked done. Leaf coverage does not substitute for glue
  coverage: every production bug in the M13 chunked pipeline was in the glue,
  with paranoid leaf coverage in place.
- **A regression test must fail before the fix.** Verify this by reverting the
  fix and watching it go red. A green test on broken code proves nothing.
- **UI** gets visual verification plus a smoke test with React Testing Library.

Additional engineering rules that block review:

- **Twin parity.** Fixing a bug in one of a paired module obliges you to check
  its twin for the same hole: `AudioRecorder` ↔ `ProcessTapRecorder`, chunk FSM ↔
  call lifecycle, `searchCalls` ↔ `findContactsByName`.
- **Degradation must be visible.** Any warn-and-continue path that affects a
  call's result must set a persistent degraded flag reachable by the UI
  (`packages/contracts/src/degraded.ts`). Log-only is forbidden.
- **i18n is total.** Every user-visible string goes through `t()` and all three
  locales, including lower layers such as `api/errors.ts`. A Russian literal on
  a UI path is a review comment.
- **CPU work over ~10 ms never runs on the async executor.** Levenshtein,
  clustering, ONNX inference and WAV reading belong in `spawn_blocking`.
- **No real time in tests.** `sleep()` for synchronizing with a background task
  is forbidden — use injectable time, `Notify`/oneshot, or `tokio::time::pause()`.
- **Trust boundaries are validated.** Any id arriving from the webview or MCP is
  validated as a UUID before it touches a file path, and the path additionally
  goes through `ensure_path_under`.
- **800 lines is measured on the resulting file**, not on your diff. Plan new
  modules to fit. Translation dictionaries (`src/i18n/{ru,en,kk}.ts`) are an
  intentional exception.

### 3. Verify locally

```bash
# Rust
cargo fmt --check
cargo check
cargo clippy -- -D warnings
cargo test

# TypeScript
pnpm -r typecheck
pnpm -r test

# UI: run the real app and check both themes on the screens you touched
pnpm tauri dev
```

"Demo" and "show me" mean a real run of the target environment — `pnpm tauri dev`
for the desktop app. A Vite-only browser preview does not count.

### 4. Coverage

CI enforces ratchets, not aspirations. Current floors:

| Scope | Gate |
|---|---|
| Frontend (`apps/desktop`) | lines/statements 69, functions 58, branches 82 (`vitest.config.ts`) |
| Rust core | `cargo llvm-cov --fail-under-lines 50` (`.github/workflows/ci.yml`) |

These are set at "actual minus a small margin" so they catch deleted tests. If
you are short on coverage, write the tests before the feature. **Lowering a
threshold requires explicit maintainer agreement** and a reason in the PR.

## Commits and pull requests

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/);
`commitlint` runs in CI and will reject anything else.

```text
<type>: <subject>

<optional body>
```

Allowed types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`,
`build`, `style`, `revert`. Header limit is 120 characters.

**The PR title must follow the same convention** — `feat(assistant): …`, not
`M15: …`. CI validates the title, and a milestone-style title fails the run.

Pull request checklist:

- [ ] Tests added or updated; a regression test was confirmed to fail before the fix
- [ ] `cargo clippy -- -D warnings` and `cargo test` pass
- [ ] `pnpm -r typecheck` and `pnpm -r test` pass
- [ ] UI changes include the design-gate alignment block and were checked in light **and** dark
- [ ] No new user-visible string bypasses `t()` in all three locales
- [ ] Linked to the relevant [`docs/ROADMAP.md`](docs/ROADMAP.md) item, if any

## Security-sensitive areas

Changes in these modules require a security review before merge. Say so in the
PR so a maintainer runs it.

| Module | Threats |
|---|---|
| `services/mcp/**` | Call content is untrusted data; instruction injection through transcripts; no network calls allowed |
| Keychain seam (`secrets.rs`) | Key leakage into the database, logs or telemetry; values belong in the system keychain only |
| Audio sidecar permissions | Recording without consent; privilege escalation in the Swift process |
| Cascade delete | Residual voice samples; incomplete cleanup of `voice_samples.source_call` |
| `local_engine/models.rs` | SHA256 is the only defense against tampered Hugging Face releases; partial-download races |
| `local_engine/{llm,stt}.rs` | Sidecar argument injection; path traversal; temp-file permissions; zombie processes on timeout |
| `capabilities/default.json` | Sidecar whitelist correctness; anchored argument validator regexes |

Never report a vulnerability in a public issue — follow [`SECURITY.md`](SECURITY.md).

## Local automation

`.claude/settings.json` wires hooks that run on file writes. They are advisory
except where noted:

- `scripts/hooks/pre-write.mjs` — **blocks** writes to signing keys, `.env*`,
  `*.key`, `*.pem`, and files that would grow past 800 lines.
- `scripts/hooks/post-write.mjs` — runs `cargo fmt` + `cargo check` on Rust,
  typecheck on TypeScript.
- `scripts/hooks/tdd-warn.mjs` — warns when source is edited without a
  neighbouring test.
- `scripts/hooks/design-gate.mjs` — warns on raw hex/oklch or legacy `--color-*`
  tokens.

They are a convenience, not a substitute for running the verification commands
above.

## Code of conduct

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## License

Contributions are licensed under the [Apache License 2.0](LICENSE), the same
license as the project. By submitting a pull request you agree that your
contribution is provided under those terms, per section 5 of the license.
