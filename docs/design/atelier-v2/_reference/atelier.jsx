/* eslint-disable */
// ─────────────────────────────────────────────────────────────
// A · ATELIER — editorial, transcript-first
// ─────────────────────────────────────────────────────────────

// Speaker colors — restrained, distinguishable on both light & dark
const SP_COLORS_A = ['#3D5BAB', '#2E8C5F', '#B86842', '#7958C7', '#3D87A4'];

function AteNav({ active = 'home' }) {
  const items = [
    { id: 'home', label: 'Главная' },
    { id: 'calls', label: 'Звонки' },
    { id: 'contacts', label: 'Контакты' },
    { id: 'settings', label: 'Настройки' },
  ];
  return (
    <aside className="ate-rail">
      <div className="ate-brand">
        Wotold<span className="ate-brand-dot">.</span>
      </div>
      {items.map((it) => (
        <button
          key={it.id}
          className={`ate-nav-item${active === it.id ? ' ate-nav-item--active' : ''}`}
        >
          {it.label}
        </button>
      ))}
      <div className="ate-rail-foot">
        v1.0.0<br />
        Локально · macOS
      </div>
    </aside>
  );
}

function AteSpeakerChip({ name, colorIdx = 0 }) {
  const initials = name
    .split(' ')
    .map((s) => s[0])
    .slice(0, 2)
    .join('');
  return (
    <span className="sp">
      <span
        className="sp-avatar"
        style={{ background: SP_COLORS_A[colorIdx % SP_COLORS_A.length] }}
      >
        {initials}
      </span>
      {name}
    </span>
  );
}

// ============================================================
// 1. Onboarding
// ============================================================
function AteOnboarding({ theme = 'light' } = {}) {
  const { accent = 'persian' } = React.useContext(window.AtelierContext || React.createContext({}));
  return (
    <div className="atelier win" data-theme={theme} data-accent={accent}>
      <WinChrome theme="atelier">
        Wotold <b>· Знакомство</b>
      </WinChrome>
      <div
        className="ate-body"
        style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}
      >
        <div style={{ width: 540, padding: '40px 0' }}>
          <div className="eyebrow" style={{ marginBottom: 14 }}>
            Шаг 02 из 03 · Владелец
          </div>
          <div className="display" style={{ marginBottom: 14 }}>
            Ваш голос — <br />
            первый.
          </div>
          <p
            className="subtitle"
            style={{ marginBottom: 36, maxWidth: 460 }}
          >
            Wotold отделяет вашу речь от речи собеседника. Расскажите, кто вы — мы запомним
            ваш голос и больше не будем спрашивать.
          </p>

          <div
            style={{
              display: 'grid',
              gridTemplateColumns: '1fr 1fr',
              gap: '24px 32px',
              marginBottom: 40,
            }}
          >
            <div className="field">
              <label className="field-label">Имя</label>
              <input className="input" defaultValue="Айдар Жунусов" />
            </div>
            <div className="field">
              <label className="field-label">Роль</label>
              <input className="input" defaultValue="Co-founder, Wotold" />
            </div>
            <div className="field" style={{ gridColumn: '1 / -1' }}>
              <label className="field-label">Краткое представление</label>
              <input
                className="input"
                defaultValue="Привет, это Айдар"
                placeholder="как вы здороваетесь"
              />
              <span
                className="muted"
                style={{ fontSize: 12, marginTop: 6, fontStyle: 'italic' }}
              >
                Поможет распознать вас на старте звонка.
              </span>
            </div>
          </div>

          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 16,
              borderTop: '1px solid var(--line-soft)',
              paddingTop: 24,
            }}
          >
            <button className="btn btn--primary">Дальше →</button>
            <button className="btn btn--quiet">Пропустить</button>
            <div style={{ marginLeft: 'auto', display: 'flex', gap: 6 }}>
              <span
                className="dot"
                style={{ background: 'var(--accent)', opacity: 1 }}
              />
              <span
                className="dot"
                style={{ background: 'var(--accent)', opacity: 1 }}
              />
              <span
                className="dot"
                style={{ background: 'var(--line)', opacity: 1 }}
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ============================================================
// 2. Home — idle
// ============================================================
function AteHome({ theme = 'light' } = {}) {
  const { accent = 'persian' } = React.useContext(window.AtelierContext || React.createContext({}));
  return (
    <div className="atelier win" data-theme={theme} data-accent={accent}>
      <WinChrome theme="atelier">
        Wotold <b>· Главная</b>
      </WinChrome>
      <div className="ate-body">
        <AteNav active="home" />
        <div className="ate-main">
          <div className="eyebrow" style={{ marginBottom: 18 }}>
            Вторник · 19 мая
          </div>
          <div className="display" style={{ marginBottom: 12 }}>
            Готов записывать.
          </div>
          <p className="subtitle" style={{ maxWidth: 540, marginBottom: 38 }}>
            Нажмите красный кружок когда начнёте звонок. Расшифровка приходит через
            10–30 секунд.
          </p>

          <div
            style={{
              display: 'flex',
              gap: 36,
              alignItems: 'center',
              marginBottom: 44,
            }}
          >
            <button className="rec-btn" aria-label="Начать запись" />
            <div>
              <div className="small-caps" style={{ marginBottom: 4 }}>
                ⌘ ⇧ R
              </div>
              <div
                style={{
                  fontFamily: 'var(--serif)',
                  fontSize: 19,
                  fontStyle: 'italic',
                  color: 'var(--muted)',
                  maxWidth: 260,
                  lineHeight: 1.45,
                }}
              >
                Или просто скажите «Wotold, запиши»
              </div>
            </div>
          </div>

          <div style={{ display: 'flex', marginBottom: 36 }}>
            <div className="stat">
              <span className="stat-value">94</span>
              <span className="stat-label">Звонков · всего</span>
            </div>
            <div className="stat">
              <span className="stat-value">12</span>
              <span className="stat-label">За неделю</span>
            </div>
            <div className="stat">
              <span className="stat-value">38<span style={{ fontSize: 18, marginLeft: 4 }}>ч</span></span>
              <span className="stat-label">В архиве</span>
            </div>
            <div className="stat">
              <span className="stat-value" style={{ color: 'var(--accent)' }}>3</span>
              <span className="stat-label">Ждут подтверждения</span>
            </div>
          </div>

          <div>
            <div
              style={{
                display: 'flex',
                alignItems: 'baseline',
                gap: 16,
                marginBottom: 14,
              }}
            >
              <span className="small-caps">Недавно</span>
              <div
                style={{ flex: 1, height: 1, background: 'var(--line-soft)' }}
              />
              <button
                className="btn btn--quiet"
                style={{ padding: 0, fontSize: 13 }}
              >
                Все звонки →
              </button>
            </div>
            {SAMPLE_CALLS.slice(0, 3).map((c, idx) => (
              <div
                key={c.id}
                style={{
                  display: 'grid',
                  gridTemplateColumns: '100px 1fr auto',
                  gap: 24,
                  padding: '14px 0',
                  borderTop:
                    idx === 0 ? 'none' : '1px dotted var(--line)',
                }}
              >
                <div
                  className="mono muted"
                  style={{ fontSize: 12, letterSpacing: '0.04em' }}
                >
                  {c.when}
                </div>
                <div>
                  <div
                    style={{
                      fontFamily: 'var(--serif)',
                      fontSize: 16,
                      marginBottom: 4,
                      letterSpacing: '-0.01em',
                    }}
                  >
                    {c.title}
                  </div>
                  <div
                    className="muted"
                    style={{
                      fontFamily: 'var(--serif)',
                      fontStyle: 'italic',
                      fontSize: 13,
                    }}
                  >
                    «{c.preview}»
                  </div>
                </div>
                <div
                  className="mono muted"
                  style={{ fontSize: 12, alignSelf: 'center' }}
                >
                  {c.dur}
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

// ============================================================
// 3. Recording — active
// ============================================================
function AteRecording({ theme = 'light' } = {}) {
  const { accent = 'persian' } = React.useContext(window.AtelierContext || React.createContext({}));
  return (
    <div className="atelier win" data-theme={theme} data-accent={accent} style={{ background: 'var(--paper)' }}>
      <WinChrome theme="atelier">
        Wotold <b>· Запись</b>
      </WinChrome>
      <div
        className="ate-body"
        style={{
          display: 'flex',
          flexDirection: 'column',
          padding: '40px 56px',
          gap: 32,
        }}
      >
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <div>
            <div
              className="eyebrow"
              style={{ marginBottom: 10, color: 'var(--signal)' }}
            >
              ● Идёт запись · Локально · Soniox
            </div>
            <div
              className="display"
              style={{
                fontFamily: 'var(--mono)',
                fontSize: 92,
                fontWeight: 400,
                letterSpacing: '0.02em',
                color: 'var(--ink)',
                lineHeight: 1,
              }}
            >
              00:14<span style={{ color: 'var(--signal)' }}>:23</span>
            </div>
          </div>
          <button className="rec-btn rec-btn--stop" aria-label="Остановить" />
        </div>

        <div
          style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            gap: 18,
            justifyContent: 'center',
          }}
        >
          <div>
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                marginBottom: 8,
              }}
            >
              <span className="small-caps">Вы · микрофон</span>
              <span
                className="mono muted"
                style={{ fontSize: 11, letterSpacing: '0.04em' }}
              >
                −12 dB
              </span>
            </div>
            <div className="wave-lane" style={{ height: 110 }}>
              <Waveform
                seed={42}
                color="var(--ink)"
                count={140}
                gap={2.5}
                width={1100}
                height={110}
              />
            </div>
          </div>

          <div
            style={{
              height: 1,
              background: 'var(--line-soft)',
              margin: '0 12px',
            }}
          />

          <div>
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                marginBottom: 8,
              }}
            >
              <span
                className="small-caps"
                style={{ color: 'var(--accent)' }}
              >
                Собеседник · системный звук
              </span>
              <span
                className="mono muted"
                style={{ fontSize: 11, letterSpacing: '0.04em' }}
              >
                −18 dB
              </span>
            </div>
            <div className="wave-lane" style={{ height: 110 }}>
              <Waveform
                seed={73}
                color="var(--accent)"
                count={140}
                gap={2.5}
                width={1100}
                height={110}
              />
            </div>
          </div>
        </div>

        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            borderTop: '1px solid var(--line-soft)',
            paddingTop: 16,
          }}
        >
          <div
            className="mono muted"
            style={{ fontSize: 11, letterSpacing: '0.06em' }}
          >
            16 кГц моно · WAV · 14 МБ записано
          </div>
          <div
            style={{
              fontFamily: 'var(--serif)',
              fontStyle: 'italic',
              color: 'var(--muted)',
              fontSize: 13,
            }}
          >
            Расшифровка начнётся автоматически
          </div>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  AteNav, AteSpeakerChip, AteOnboarding, AteHome, AteRecording,
  SP_COLORS_A,
});
