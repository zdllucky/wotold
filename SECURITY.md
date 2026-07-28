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

**The signing key does not exist yet.** `apps/desktop/src-tauri/tauri.conf.json`
contains a literal placeholder in place of the public key, so update verification
fails closed and no update is ever installed. The manifest request still goes out
on launch. Treat signed auto-update as not yet shipped: reports about signature
bypass are premature until the key is generated.

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

## Threat model in one paragraph

Wotold is local-only. There is no server, no account, and no cloud storage — the
cloud segment was removed in version 0.3. Two outbound network flows exist, and
neither carries user data: the one-time model download (`huggingface.co`, plus
`github.com` for the WeSpeaker embedder), and an anonymous version-manifest GET
to `github.com` on every launch. Any finding that shows call audio, transcripts,
or embeddings leaving the device through any path is by definition a
high-severity report.
