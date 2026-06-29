# Migration Map · Page by Page

> ⚠️ **LEGACY (Atelier v2).** Этот документ описывает старый дизайн (Atelier v2),
> полностью заменённый редизайном **Wotold v2 (B18)**. Часть упоминаемых здесь
> экранов (напр. HomePage) и классов больше не существует. Действующий канон —
> [`docs/design/wotold-v2/README.md`](../wotold-v2/README.md) + код (`wk.css` /
> `components.css` / `tokens.css`). Файл остаётся только для истории.

This is the developer's task list. Each section maps old JSX → new JSX + classes.
Apply in order; **don't skip Foundation**.

---

## 0. Foundation

See `README.md` §"Implementation plan" → Step 1. Once done, your `<html>` should
carry `data-theme` and `data-accent` attributes, and `tokens.css` / `wotold.css`
are imported.

---

## 1. App shell · `apps/desktop/src/App.tsx`

### Before
```tsx
<nav className="topnav">
  <span className="topnav-brand">Wotold</span>
  <div className="topnav-tabs">
    {NAV.map(item => (
      <button className={`topnav-tab ${active ? 'topnav-tab--active' : ''}`}>
        <span className="topnav-tab-icon">{item.icon}</span>
        <span className="topnav-tab-label">{item.label}</span>
      </button>
    ))}
  </div>
</nav>
<main className="app">{page-content}</main>
```

### After
```tsx
<div className="app-shell">
  <aside className="app-rail">
    <div className="app-brand">Wotold<span className="app-brand-dot">.</span></div>
    {NAV.map(item => (
      <button className={`nav-item ${active ? 'nav-item--active' : ''}`}>
        {item.label}
      </button>
    ))}
    <div className="app-rail-foot">v1.0.0<br/>Локально · macOS</div>
  </aside>
  <main className="app-main">{page-content}</main>
</div>
```

**Drop:** all emoji icons (🎙 📞 👥 ⚙). Text-only nav with the bordeaux indicator bar carries enough signal.

---

## 2. HomePage · `apps/desktop/src/pages/HomePage.tsx`

Full sample provided as `HomePage.tsx` in this handoff. Key replacements:

| Old | New |
|---|---|
| `<div className="home-hero">` + `<h1 className="home-title">` | `<header>` + `<h1 className="display">` |
| `<p className="home-subtitle text-muted">` | `<p className="subtitle">` |
| `<div className="home-stats">` + `.home-stat-card` | `<div className="stat-row">` + `.stat` |
| `<Button variant="record" size="lg" pill>` | `<button className="rec-btn" />` (round, see below) |
| `<StatusDot tone="danger" size="lg" pulse />` | inline `<span className="dot dot--signal dot--pulse" />` |
| `<Card variant="raised">` | `<div className="card card--raised">` |
| `.record-saved-card` | `<div className="card">` |
| `.consent-card` | full-screen `<div className="modal-backdrop"><div className="index-card">` |

**The record button is round, not a pill.** It's `<button className="rec-btn">`, a 108×108 circle with a white dot at center. Recording state swaps to `className="rec-btn rec-btn--stop"` (square inner shape). The pulsing `<StatusDot tone="danger">` becomes the button itself + a separate timer next to it.

---

## 3. CallsPage · `apps/desktop/src/pages/CallsPage.tsx`

The current implementation appears to render cards in a grid. Replace with a typeset list grouped by month.

```tsx
<div>
  {/* Header */}
  <div style={{ display: 'flex', alignItems: 'flex-end', gap: 24, marginBottom: 26 }}>
    <div className="title" style={{ fontSize: 36 }}>Звонки</div>
    <div style={{ flex: 1 }}>
      <input className="input" placeholder="Найти в расшифровках…" />
    </div>
    {/* Filter pills — use .btn btn--ghost / btn--quiet */}
  </div>

  {/* Total */}
  <div className="small-caps" style={{ marginBottom: 18 }}>
    {calls.length} звонков · {totalHours} ч
  </div>

  {/* Groups */}
  {monthGroups.map(group => (
    <div key={group.label} style={{ marginBottom: 32 }}>
      <div style={{ display: 'grid', gridTemplateColumns: '120px 1fr', gap: 32 }}>
        <div className="small-caps" style={{ paddingTop: 14 }}>{group.label}</div>
        <div>
          {group.calls.map((c, idx) => (
            <div
              key={c.id}
              style={{
                display: 'grid',
                gridTemplateColumns: '64px 1fr 200px 70px',
                gap: 20,
                padding: '16px 0',
                borderTop: idx === 0 ? 'none' : '1px solid var(--line-soft)',
                alignItems: 'baseline',
              }}
            >
              <div className="mono muted">{formatDay(c.started_at)}</div>
              <div>
                <div style={{ fontFamily: 'var(--font-serif)', fontSize: 17 }}>
                  {c.title}
                </div>
                {c.preview && (
                  <div className="muted" style={{
                    fontFamily: 'var(--font-serif)',
                    fontStyle: 'italic',
                    fontSize: 14,
                  }}>
                    «{c.preview}»
                  </div>
                )}
              </div>
              {/* Stacked speaker avatars — see snippet below */}
              <div className="sp-stack">
                {c.speakers.slice(0, 3).map((s, i) => (
                  <span
                    key={i}
                    className="sp-avatar"
                    style={{ background: `var(--sp-${(i % 5) + 1})` }}
                  >
                    {initials(s)}
                  </span>
                ))}
              </div>
              <div className="mono muted" style={{ textAlign: 'right' }}>
                {formatDuration(c.duration_sec)}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  ))}
</div>
```

---

## 4. CallDetailPage · `apps/desktop/src/pages/CallDetailPage.tsx`

### Header

```tsx
<button className="btn btn--quiet" onClick={onBack}>← Все звонки</button>

<div className="small-caps" style={{ marginBottom: 8 }}>
  {formatDateLong(call.started_at)} · {formatDuration(call.duration_sec)}
</div>
<h1 className="title" style={{ fontSize: 36 }}>{call.title}</h1>

<div style={{ display: 'flex', gap: 8, marginTop: 14 }}>
  {participants.map((p, i) => (
    <span className="sp" key={p.id}>
      <span className="sp-avatar" style={{ background: `var(--sp-${(i % 5) + 1})` }}>
        {initials(p.name)}
      </span>
      {p.name}
    </span>
  ))}
</div>
```

### Tabs

```tsx
<div className="tabs">
  <button className={`tab ${tab === 'transcript' ? 'tab--active' : ''}`}>Расшифровка</button>
  <button className={`tab ${tab === 'recap' ? 'tab--active' : ''}`}>Рекап</button>
  <button className={`tab ${tab === 'tasks' ? 'tab--active' : ''}`}>Задачи · {tasks.length}</button>
  <button className={`tab ${tab === 'people' ? 'tab--active' : ''}`}>Участники</button>
</div>
```

### Transcript (the hero)

```tsx
<div className="transcript">
  {turns.map(t => (
    <div className="transcript-row" key={t.id}>
      <div
        className="transcript-speaker"
        style={{ color: `var(--sp-${(t.speakerIdx % 5) + 1})` }}
      >
        {t.speakerName}
      </div>
      <div className="transcript-text">{t.text}</div>
      <div className="transcript-time">{formatTime(t.startMs)}</div>
    </div>
  ))}
</div>
```

### Recap

Two-column dossier layout. Use a basic CSS grid:

```tsx
<div style={{ display: 'grid', gridTemplateColumns: '1fr 280px', gap: 56 }}>
  {/* Main column */}
  <div>
    <section style={{ marginBottom: 32 }}>
      <div className="small-caps">Резюме</div>
      <p style={{ fontFamily: 'var(--font-serif)', fontSize: 19, lineHeight: 1.55 }}>
        {recap.summary}
      </p>
    </section>

    <section style={{ marginBottom: 32 }}>
      <div className="small-caps">Ключевые моменты</div>
      <ol style={{ listStyle: 'none', padding: 0 }}>
        {recap.key_points.map((point, i) => (
          <li key={i} style={{ display: 'flex', gap: 14, padding: '6px 0', borderBottom: '1px solid var(--line-soft)' }}>
            <span className="mono" style={{ color: 'var(--accent)', minWidth: 22 }}>0{i + 1}</span>
            <span style={{ fontFamily: 'var(--font-serif)', fontSize: 16 }}>{point}</span>
          </li>
        ))}
      </ol>
    </section>

    <section>
      <div className="small-caps">Задачи · {tasks.length}</div>
      {tasks.map(t => <TaskRow key={t.id} task={t} />)}
    </section>
  </div>

  {/* Sidebar */}
  <aside>
    <div className="small-caps">Метаданные</div>
    {/* key-value list */}
  </aside>
</div>
```

---

## 5. Speaker confirmation · `apps/desktop/src/pages/SpeakersSection.tsx`

Modal becomes an `.index-card` overlay. Each speaker gets a card with:
- Speaker bubble (avatar at speaker color, "S2" or initial)
- One sample line in italic Source Serif 4
- Best-match contact + `.conf` bar
- Three buttons: `Confirm · NAME` (`.btn--primary`), `Reject` (`.btn--ghost`), `+ New contact` (`.btn--ghost`)

```tsx
<div className="modal-backdrop">
  <div className="index-card">
    <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
      <div className="small-caps">Голос {idx + 1} из {total}</div>
      <div className="small-caps muted">из · {call.title}</div>
    </div>

    <h2 className="title" style={{ fontSize: 28, marginBottom: 28 }}>Кто этот голос?</h2>

    {/* Sample line + waveform — use any small waveform component */}
    {/* Best match */}
    <div className="small-caps" style={{ marginBottom: 10 }}>Похоже на</div>
    <div style={{ display: 'flex', alignItems: 'center', gap: 14, marginBottom: 24 }}>
      <span className="sp-avatar" style={{ width: 38, height: 38, fontSize: 12, background: `var(--sp-${speakerIdx + 1})` }}>
        {initials(match.name)}
      </span>
      <div style={{ flex: 1 }}>
        <div style={{ fontFamily: 'var(--font-serif)', fontSize: 17 }}>{match.name}</div>
        <div className="muted" style={{ fontSize: 12 }}>{match.role} · {match.prior_calls} предыдущих звонков</div>
      </div>
      <div style={{ width: 120 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
          <span className="small-caps">Уверенность</span>
          <span className="mono">{Math.round(match.score * 100)}%</span>
        </div>
        <div className="conf"><div className="conf-fill" style={{ width: `${match.score * 100}%` }} /></div>
      </div>
    </div>

    <div style={{ display: 'flex', gap: 10 }}>
      <button className="btn btn--primary" style={{ flex: 1, justifyContent: 'center' }} onClick={() => confirm(match)}>
        ✓ Да, это {match.name.split(' ')[0]}
      </button>
      <button className="btn btn--ghost" onClick={reject}>Не он/она</button>
      <button className="btn btn--ghost" onClick={createNew}>Новый контакт</button>
    </div>
  </div>
</div>
```

---

## 6. Contacts · `apps/desktop/src/pages/ContactsPage.tsx`

Two-column layout: 320px list + flex detail.

- List items: 30×30 colored avatar + name (serif) + role (DM Sans muted).
- Active item: `background: var(--bg-2)` plus the `.nav-item--active` style trick (left bar).
- Detail header: 76×76 avatar (rounded square), `.display` name, `.subtitle` role.
- Stats row: 3× `.stat` (calls, recorded duration, voice samples).
- Contact fields: 2-column grid with `.small-caps` labels.
- Voice samples table: rows with src/wave/duration/quality/×.

---

## 7. Settings · `apps/desktop/src/pages/SettingsPage.tsx`

Sub-sidebar within `.app-main`:

```tsx
<div style={{ display: 'flex', minHeight: '100%' }}>
  <div style={{ width: 220, padding: '32px 22px', borderRight: '1px solid var(--line-soft)' }}>
    <div className="small-caps" style={{ marginBottom: 14 }}>Настройки</div>
    {SETTINGS_NAV.map(s => (
      <button className={`nav-item ${section === s.id ? 'nav-item--active' : ''}`}>
        {s.label}
      </button>
    ))}
  </div>
  <div style={{ flex: 1, padding: '32px 44px' }}>{renderSection(section)}</div>
</div>
```

Each section:
- Page eyebrow: `Настройки · ${section.label}`
- `<div className="display" style={{ fontSize: 40 }}>` headline (one per section)
- `.subtitle` lede
- Form rows: `<div className="field">` + `<label className="field-label">` + `<input className="input">`

**Add a new section: "Внешний вид"** — see `README.md` →"Theme switching" snippet. Place it near the top of the sidebar (after Account).

---

## 8. Onboarding · `apps/desktop/src/pages/OnboardingPage.tsx`

Centred 3-step flow. No nav rail visible.

```tsx
<div className="modal-backdrop">
  <div style={{ width: 540, padding: '40px 0' }}>
    <div className="small-caps" style={{ marginBottom: 14 }}>
      Шаг 0{step} из 03 · {STEP_LABEL[step]}
    </div>
    <h1 className="display" style={{ marginBottom: 14 }}>{STEP_HEADLINE[step]}</h1>
    <p className="subtitle" style={{ marginBottom: 36 }}>{STEP_LEDE[step]}</p>

    {/* Fields per step */}

    <div style={{ display: 'flex', alignItems: 'center', gap: 16, borderTop: '1px solid var(--line-soft)', paddingTop: 24 }}>
      <button className="btn btn--primary" onClick={next}>Дальше →</button>
      <button className="btn btn--quiet" onClick={skip}>Пропустить</button>
      <div style={{ marginLeft: 'auto', display: 'flex', gap: 6 }}>
        {[1,2,3].map(i => <span className="dot" style={{ background: i <= step ? 'var(--accent)' : 'var(--line)' }} />)}
      </div>
    </div>
  </div>
</div>
```

---

## 9. Buttons · `apps/desktop/src/ui/Button.tsx`

Shrink the existing component. Match variants:

| Old `variant` | New className |
|---|---|
| `primary`   | `btn btn--primary` |
| `secondary` | `btn btn--ghost` |
| `ghost`     | `btn btn--quiet` |
| `record`    | replace with `<button className="rec-btn">` (round, not pill) |
| `danger`    | `btn btn--danger` |

Sizes: `sm` → `btn--sm`, `md` → default, `lg` → `btn--lg`. Remove `pill` prop — use the same shape always.

---

## 10. Cleanup

After all pages migrate:
1. Delete `apps/desktop/src/styles/pages.css` (29 KB!) — almost everything in it is now in `wotold.css` or inline.
2. Audit `global.css` — keep only resets that aren't already in `wotold.css`.
3. Update `DesignSystemPage.tsx` to show the new tokens — useful for QA.
4. `pnpm -r typecheck && pnpm -r test`.
