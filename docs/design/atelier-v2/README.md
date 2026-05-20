# Wotold · Atelier Redesign — Handoff Package

**Project:** Wotold desktop (Tauri 2, TS + Rust + Swift sidecar)
**Target codebase:** `apps/desktop/src/` in `zdllucky/wotold`
**Design direction:** **Atelier v2** — editorial, transcript-first
**Default accent:** **Bordeaux** (oxblood wine, #7E1F2A → #D45A6B dark)
**Themes:** light + dark (orthogonal to accent — any combination works)
**Fonts:** Source Serif 4 (content) · DM Sans (UI) · JetBrains Mono (numerals)

---

## About these files

The HTML mocks in the parent project (`index.html`, `atelier*.jsx`) are **design references** — prototypes showing intended look and behaviour, not production code to copy verbatim. The task is to **port these designs into the existing Wotold codebase** (`apps/desktop`), preserving all current logic (recording flow, hotkeys, API calls, consent gates, updater, etc.) and only replacing JSX markup + styling.

**Fidelity:** high. Exact hex values, exact spacing, exact typography are in `tokens.css` / `wotold.css`. Match them.

---

## What's in this folder

| File | Purpose | Where it goes |
|---|---|---|
| `tokens.css`     | All design tokens — colors, type scale, spacing, radii, shadows. Both themes + 3 accent swatches. | **Replaces** `apps/desktop/src/styles/tokens.css` |
| `wotold.css`     | Component class library (`.rec-btn`, `.transcript-row`, `.sp`, `.card`, `.tabs`, `.btn`, `.field`, etc). | New file → `apps/desktop/src/styles/wotold.css` |
| `fonts.css`      | Font-face setup — Google CDN or self-hosted variant. | New file → `apps/desktop/src/styles/fonts.css` |
| `useTheme.tsx`   | React hook + `<ThemeProvider>`. Persists choice via `api/settings`, applies to `<html>`, syncs with system. | New file → `apps/desktop/src/theme/useTheme.tsx` |
| `App.tsx`        | Sample rewrite of `apps/desktop/src/App.tsx` showing the new shell + nav rail. | Reference — replace existing `App.tsx` |
| `HomePage.tsx`   | Sample rewrite of `apps/desktop/src/pages/HomePage.tsx`. Same logic, new JSX + classes. | Reference — replace existing `HomePage.tsx` |
| `MIGRATION.md`   | Page-by-page migration map. Use as the task list. | Keep for reference |

---

## Implementation plan (PDCA, follows §17/W3 of the паспорт)

### Step 1 — Foundation (~30 min)

1. Drop `tokens.css`, `wotold.css`, `fonts.css` into `apps/desktop/src/styles/`.
2. Import them in this order in `apps/desktop/src/main.tsx`:
   ```ts
   import './styles/fonts.css';
   import './styles/tokens.css';
   import './styles/wotold.css';
   import './styles/global.css';   // existing — keep, will shrink later
   import './styles/pages.css';    // existing — gradually replaced
   ```
3. Drop `useTheme.tsx` into `apps/desktop/src/theme/`.
4. Wrap `<App>` in `<ThemeProvider>` (see `App.tsx`).
5. Add two new settings keys to `apps/desktop/src/api/settings.ts`:
   ```ts
   export const SETTINGS_KEYS = {
     // ...existing
     UI_THEME:  'ui.theme',   // 'light' | 'dark' | 'system'
     UI_ACCENT: 'ui.accent',  // 'bordeaux' | 'persian' | 'ink'
   } as const;
   ```
6. `pnpm --filter @wotold/desktop typecheck && pnpm --filter @wotold/desktop dev` — verify it boots, page might still look like before (using `pages.css`).

**Acceptance:** app boots with no console errors, `document.documentElement.getAttribute('data-theme')` reads `'light'` or `'dark'`, `data-accent` reads `'bordeaux'`.

### Step 2 — Shell + nav (~20 min)

Replace `apps/desktop/src/App.tsx` with the version in this folder (or apply the diff). Key changes:
- Topnav (`.topnav`) → sidebar rail (`.app-rail` + `.nav-item`).
- Emoji icons removed — text-only nav with active-indicator bar.
- Main wrapped in `.app-shell` + `.app-main`.
- `<ThemeProvider>` wraps the app.

**Acceptance:** sidebar shows 4 nav items, active item has a bordeaux indicator bar.

### Step 3 — HomePage (~30 min)

Replace `apps/desktop/src/pages/HomePage.tsx` with the version in this folder. The version preserves **all** existing logic:
- `useEffect` for updater check, recording state, consent timestamp, recent calls
- Hotkey `⌘⇧R`
- Consent gate before first recording
- Update prompt with `applyUpdate`
- All API contracts identical

What changed: JSX structure + class names. No new dependencies.

**Acceptance:** recording starts/stops, consent modal appears once on first record, hotkey works, recent calls list opens.

### Step 4 — CallDetailPage transcript (~45 min)

The transcript view is the **hero screen** — give it the most care. See `MIGRATION.md` for the exact markup. Key bits:

```tsx
<div className="transcript">
  {turns.map(turn => (
    <div className="transcript-row" key={turn.id}>
      <div
        className="transcript-speaker"
        style={{ color: SPEAKER_COLORS[turn.speakerIdx] }}
      >
        {turn.speakerName}
      </div>
      <div className="transcript-text">{turn.text}</div>
      <div className="transcript-time">{formatTime(turn.startMs)}</div>
    </div>
  ))}
</div>
```

Speaker colors come from CSS vars `--sp-1` through `--sp-5` (defined in tokens.css, adjusted for dark theme).

### Step 5 — Remaining pages (~2–3 h total)

In order of importance:
- **`CallsPage`** — replace card list with the date-grouped layout from `MIGRATION.md`.
- **`SpeakersSection`** — the speaker confirmation modal (`.index-card` + `.conf` bar).
- **`ContactsPage`** — two-column list + detail, with voice fingerprints.
- **`SettingsPage`** — sidebar within sidebar; add a new **"Внешний вид"** section that uses `useTheme()` to expose theme + accent pickers.
- **`OnboardingPage`** — 3-step flow, big `.display` heading, `.input` fields.

### Step 6 — Cleanup (~30 min)

1. Delete or shrink `apps/desktop/src/styles/pages.css` — most of it becomes redundant.
2. Audit `apps/desktop/src/ui/Button.tsx`, `Card.tsx`, `Badge.tsx`, `Empty.tsx` — most can become thin wrappers that just spread `className="btn btn--primary"` etc.
3. Update `DesignSystemPage.tsx` to showcase the new tokens (this is dev-only but useful for QA).
4. Run `pnpm -r typecheck && pnpm -r test`.
5. `/code-review` on the diff, fix CRITICAL/HIGH.

---

## Theme switching — for SettingsPage

Add this to `SettingsPage.tsx` as a new section near the top (or to `AccountSection.tsx`):

```tsx
import { useTheme } from '../theme/useTheme';

export function AppearanceSection() {
  const { theme, setTheme, accent, setAccent } = useTheme();

  return (
    <section>
      <h2 className="title">Внешний вид</h2>

      <div className="field" style={{ marginBottom: 24 }}>
        <label className="field-label">Тема</label>
        <div style={{ display: 'flex', gap: 6, marginTop: 8 }}>
          {(['light', 'dark', 'system'] as const).map(t => (
            <button
              key={t}
              type="button"
              className={`btn ${theme === t ? 'btn--primary' : 'btn--ghost'}`}
              onClick={() => setTheme(t)}
            >
              {t === 'light' ? 'Светлая' : t === 'dark' ? 'Тёмная' : 'Системная'}
            </button>
          ))}
        </div>
      </div>

      <div className="field">
        <label className="field-label">Акцентный цвет</label>
        <div style={{ display: 'flex', gap: 6, marginTop: 8 }}>
          {(['bordeaux', 'persian', 'ink'] as const).map(a => (
            <button
              key={a}
              type="button"
              className={`btn ${accent === a ? 'btn--primary' : 'btn--ghost'}`}
              onClick={() => setAccent(a)}
            >
              {a === 'bordeaux' ? 'Бордо' : a === 'persian' ? 'Кобальт' : 'Графит'}
            </button>
          ))}
        </div>
      </div>
    </section>
  );
}
```

---

## Design tokens — quick reference

### Colors (Light theme, Bordeaux accent)

| Token | Hex | Use |
|---|---|---|
| `--bg`           | `#F4F4F2` | Page background |
| `--bg-2`         | `#ECECE8` | Subtle inset / hover |
| `--paper`        | `#FCFCFA` | Sidebar / chrome |
| `--surface`      | `#FFFFFF` | Cards, inputs |
| `--line`         | `#E1E0DA` | Dividers, borders |
| `--line-soft`    | `#ECEAE3` | Soft dividers (transcript rows) |
| `--line-strong`  | `#C8C7C0` | Input borders |
| `--ink`          | `#14151A` | Primary text |
| `--ink-2`        | `#2A2B30` | Secondary text |
| `--muted`        | `#6B6C72` | Muted text |
| `--subtle`       | `#9C9D9F` | Subtle text |
| `--accent`       | `#7E1F2A` | Bordeaux |
| `--accent-hover` | `#8E2536` | |
| `--accent-soft`  | `#F2DCDF` | |
| `--accent-fg`    | `#FFFFFF` | Text on accent |
| `--signal`       | `#DC2626` | **Record / danger only** |
| `--signal-soft`  | `#FCE7E7` | |
| `--sp-1`..`--sp-5` | see tokens.css | Speaker thread colors |

### Typography

| Class | Family | Size | Weight | Use |
|---|---|---|---|---|
| `.display`   | Source Serif 4 | 54px | 400 | Hero (1 per page) |
| `.title`     | Source Serif 4 | 25px | 500 | Section title |
| `.subtitle`  | Source Serif 4 | 17px | 400 | Lead paragraph |
| `.transcript-text` | Source Serif 4 | 17px | 400 | Spoken content |
| body         | DM Sans        | 14.7px | 400 | UI default |
| `.eyebrow`, `.small-caps` | DM Sans | 11px | 600 caps | Labels |
| `.mono` (timestamps, IDs) | JetBrains Mono | 11–13px | 400 | Numerals, codes |

### Spacing

Use `--space-1` through `--space-9` (4px base × 1/2/3/4/5/6/7/9/12).

### Critical rule

**Red (`--signal`) is reserved for recording state and destructive actions.** Do not use it for active nav, hover states, links, or anything else. The bordeaux accent is for everything else.

---

## What NOT to change

- All Rust commands (`startRecording`, `applyUpdate`, `getRecordingState`, etc.) — design preserves the existing API surface.
- `apps/desktop/src/api/*.ts` — leave alone (one addition: two settings keys, listed above).
- `apps/desktop/src-tauri/` — no changes.
- `services/proxy/` — no changes.
- `packages/contracts/` — no changes.
- Diarization / matching algorithms — design surfaces are presentational.

## Verification before merge

Per паспорт §17 / `CLAUDE.md`:

```bash
pnpm --filter @wotold/desktop typecheck
pnpm --filter @wotold/desktop test
# run /code-review on the diff before commit
```

Sensitive modules (keychain, MCP, proxy) are not touched by this redesign — `/security-scan` is not required, but no harm in running it.
