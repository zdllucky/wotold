# Security Policy

Wotold records phone and meeting audio and keeps the transcripts on the user's
machine. A vulnerability here does not leak a database row — it leaks somebody's
conversation. Reports are taken seriously.

## Reporting a vulnerability

**Do not open a public issue.**

Use GitHub's private vulnerability reporting:
[github.com/zdllucky/wotold/security/advisories/new](https://github.com/zdllucky/wotold/security/advisories/new).
This creates a private advisory visible only to the maintainer.

Please include:

- Affected version, macOS version, and model preset (Light / Balanced / Quality) if relevant
- What an attacker gains — read access to audio, code execution, privilege escalation, and so on
- Reproduction steps, and a proof of concept if you have one
- **No real call content.** Redact transcripts, audio and contact names; a synthetic reproduction is always preferable

### What to expect

This is a single-maintainer project, not a company with a security team. There is
no SLA and no bug bounty. In practice:

- Acknowledgement within about a week
- An assessment and a plan, or an explanation of why the report is out of scope
- Credit in the advisory and release notes, unless you prefer to stay anonymous

Coordinated disclosure is appreciated: please give the fix a chance to ship
before publishing.

## Supported versions

Only the latest release receives fixes. The project is pre-1.0 and there are no
maintenance branches.

Updates are designed to be delivered through the Tauri updater and minisign-signed.

The signing keypair exists and the public key is committed in
`apps/desktop/src-tauri/tauri.conf.json`. Signing is not yet active in CI: until
the private key is configured as a repository secret, released builds carry no
signature and update verification fails closed — the manifest request goes out on
launch, but nothing installs. Signature-bypass reports are in scope only once a
signed release has actually shipped.

## Scope

### In scope

- **`services/mcp/**`** — the MCP server is read-only by design and makes no
  network calls. Call content is **untrusted data**: instruction injection
  through a transcript or recap that causes an MCP client to take an action is
  in scope, as is any write capability appearing in the server.
- **`local_engine/models.rs`** — model integrity. SHA256 verification is the only
  barrier against a tampered Hugging Face artifact; partial-download races and
  cache-poisoning paths are in scope.
- **`local_engine/{llm,stt}.rs`** and **`capabilities/default.json`** — sidecar
  argument injection, path traversal, temp-file exposure, unanchored validator
  regexes.
- **Audio capture and permissions** — any path that starts a recording without
  the user's action, or that escalates privileges through the Swift sidecar.
- **Data at rest and deletion** — residual audio or voice samples after a call or
  contact is deleted; anything that writes secrets into the database, logs or
  telemetry.
- **Path handling** — any id from the webview or MCP that reaches a filesystem
  path without UUID validation and `ensure_path_under`.
- **The updater** — signature bypass, downgrade attacks, manifest tampering.

### Out of scope

- **Deliberate limitations** listed in section 12 of
  [`docs/ПАСПОРТ_ПРОЕКТА.md`](docs/ПАСПОРТ_ПРОЕКТА.md). In particular, **R6**:
  macOS builds are not Apple-notarized in the MVP, so Gatekeeper warns on first
  launch. That is a known, accepted state, not a vulnerability.
- **Model output quality.** Local LLM summaries are weaker than cloud ones (R12)
  and the UI says so. A wrong summary is not a security issue.
- Attacks requiring an already-compromised machine, physical access, or root.
- Third-party model content. Models are downloaded from Hugging Face at runtime
  and are not distributed by this project — see [`NOTICE`](NOTICE). Report issues
  in a model to its upstream author.
- Vulnerabilities in the GitHub Pages website that do not affect the application.
  It is static, ships no third-party JavaScript, and stores nothing.

## What counts as reachable

Dependency scanners report against the lockfile, which records far more than
this project ships. Three facts settle most of what they surface, and they are
recorded here so that neither a researcher nor a future maintainer has to derive
them again during the next wave of alerts.

**No `node_modules` reaches a user.** The desktop app's frontend is bundled by
Vite into the Rust binary (`tauri.conf.json` → `frontendDist`); its sidecars are
native executables. The website is pre-rendered static output. The MCP server is
plain `tsc` output and is not part of any release asset — it exists only for
people who clone the repository. So build and test tooling — vitest, vite,
esbuild, Babel, jsdom and everything under them — contributes nothing to a
distributed artifact, whatever its advisory says.

**The MCP server speaks stdio and nothing else.** `services/mcp/src/server.ts`
constructs exactly one transport, `StdioServerTransport`. The SDK also ships
HTTP transports, and those drag in `hono`, `express` and `body-parser` — every
advisory against them concerns HTTP surface: CORS reflection, body-limit bypass,
static-file path traversal, serverless adapters. That code sits on disk and is
never loaded into the process. The server likewise parses no URIs and opens no
sockets; it reads a local SQLite file. Alerts of this class are dismissed with
that reasoning rather than left open, and they return automatically if a fixed
version is published.

**The Linux graph never compiles.** `Cargo.lock` is target-agnostic and records
the union of all platforms, so GTK, `glib`, `webkit2gtk` and friends appear in
it. They arrive through `tray-icon` and are gated behind
`cfg(target_os = "linux")`; releases build `aarch64-apple-darwin` only.
`cargo tree -i glib --target aarch64-apple-darwin` prints nothing.

What this does **not** excuse: anything whose output reaches the published
website, anything in the Rust dependency graph that does compile on macOS, and
anything reachable from the app at runtime. CI reports `pnpm audit --prod` on
every run for exactly that reason.

## Threat model in one paragraph

Wotold is local-only. There is no server, no account, and no cloud storage — the
cloud segment was removed in version 0.3. Two outbound network flows exist, and
neither carries user data: the one-time model download (`huggingface.co`, plus
`github.com` for the WeSpeaker embedder), and an anonymous version-manifest GET
to `github.com` on every launch. Any finding that shows call audio, transcripts,
or embeddings leaving the device through any path is by definition a
high-severity report.
