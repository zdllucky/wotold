/* eslint-disable */
// ─────────────────────────────────────────────────────────────
// A · ATELIER — part 2 (calls list, detail, speakers, contacts, settings)
// ─────────────────────────────────────────────────────────────

// ============================================================
// 4. Calls list — document style
// ============================================================
function AteCallsList({ theme = 'light' } = {}) {
  const { accent = 'persian' } = React.useContext(window.AtelierContext || React.createContext({}));
  // group by month label
  const groups = [
    {
      label: 'Май 2026',
      items: SAMPLE_CALLS,
    },
  ];

  return (
    <div className="atelier win" data-theme={theme} data-accent={accent}>
      <WinChrome theme="atelier">
        Wotold <b>· Звонки</b>
      </WinChrome>
      <div className="ate-body">
        <AteNav active="calls" />
        <div className="ate-main">
          <div
            style={{
              display: 'flex',
              alignItems: 'flex-end',
              gap: 24,
              marginBottom: 26,
            }}
          >
            <div className="title" style={{ fontSize: 36 }}>
              Звонки
            </div>
            <div
              style={{
                flex: 1,
                borderBottom: '1px solid var(--line)',
                paddingBottom: 6,
              }}
            >
              <input
                className="input"
                placeholder="Найти в расшифровках…"
                style={{ borderBottom: 'none', fontSize: 15, padding: 0 }}
              />
            </div>
            <div style={{ display: 'flex', gap: 6 }}>
              {['Все', 'Сегодня', 'Неделя'].map((f, i) => (
                <button
                  key={f}
                  className={`btn ${i === 0 ? 'btn--ghost' : 'btn--quiet'}`}
                  style={{ padding: '6px 12px', fontSize: 12 }}
                >
                  {f}
                </button>
              ))}
            </div>
          </div>

          <div className="small-caps" style={{ marginBottom: 18 }}>
            94 звонка · 38 ч
          </div>

          {groups.map((g) => (
            <div key={g.label} style={{ marginBottom: 32 }}>
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: '120px 1fr',
                  gap: 32,
                }}
              >
                <div>
                  <div
                    className="small-caps"
                    style={{ paddingTop: 14, position: 'sticky', top: 0 }}
                  >
                    {g.label}
                  </div>
                </div>
                <div>
                  {g.items.map((c, idx) => (
                    <div
                      key={c.id}
                      style={{
                        display: 'grid',
                        gridTemplateColumns: '64px 1fr 200px 70px',
                        gap: 20,
                        padding: '16px 0',
                        borderTop:
                          idx === 0 ? 'none' : '1px dotted var(--line)',
                        alignItems: 'baseline',
                      }}
                    >
                      <div
                        className="mono muted"
                        style={{
                          fontSize: 11,
                          letterSpacing: '0.04em',
                          paddingTop: 4,
                        }}
                      >
                        {c.when.split(' · ')[0].replace(/^Сегодня/, '19')
                          .replace(/^Вчера/, '18')
                          .replace(/^15 мая/, '15')
                          .replace(/^13 мая/, '13')
                          .replace(/^10 мая/, '10')
                          .replace(/^8 мая/, '08')}
                      </div>
                      <div>
                        <div
                          style={{
                            fontFamily: 'var(--serif)',
                            fontSize: 17,
                            marginBottom: 4,
                            letterSpacing: '-0.01em',
                            color: 'var(--ink)',
                            display: 'flex',
                            alignItems: 'center',
                            gap: 8,
                          }}
                        >
                          {c.title}
                          {c.status === 'processing' && (
                            <span
                              className="mono"
                              style={{
                                fontSize: 9,
                                background: 'var(--accent-soft)',
                                color: 'var(--accent)',
                                padding: '2px 6px',
                                borderRadius: 3,
                                letterSpacing: '0.12em',
                                textTransform: 'uppercase',
                              }}
                            >
                              распознаём
                            </span>
                          )}
                        </div>
                        <div
                          className="muted"
                          style={{
                            fontFamily: 'var(--serif)',
                            fontStyle: 'italic',
                            fontSize: 14,
                            lineHeight: 1.4,
                          }}
                        >
                          «{c.preview}»
                        </div>
                      </div>
                      <div
                        style={{
                          display: 'flex',
                          gap: 4,
                          flexWrap: 'wrap',
                          alignItems: 'center',
                        }}
                      >
                        {c.speakers.slice(0, 3).map((s, i) => (
                          <span
                            key={i}
                            className="sp-avatar"
                            style={{
                              background: SP_COLORS_A[i % 5],
                              width: 24,
                              height: 24,
                              marginLeft: i === 0 ? 0 : -8,
                              border: '2px solid var(--bg)',
                              fontSize: 9,
                            }}
                          >
                            {s
                              .split(' ')
                              .map((w) => w[0])
                              .slice(0, 2)
                              .join('')}
                          </span>
                        ))}
                        {c.speakers.length > 3 && (
                          <span
                            className="mono muted"
                            style={{ fontSize: 11, marginLeft: 4 }}
                          >
                            +{c.speakers.length - 3}
                          </span>
                        )}
                      </div>
                      <div
                        className="mono muted"
                        style={{
                          fontSize: 12,
                          textAlign: 'right',
                          letterSpacing: '0.04em',
                        }}
                      >
                        {c.dur}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ============================================================
// 5. Call detail — Transcript
// ============================================================
function AteCallDetailTranscript({ theme = 'light' } = {}) {
  const { accent = 'persian' } = React.useContext(window.AtelierContext || React.createContext({}));
  return (
    <div className="atelier win" data-theme={theme} data-accent={accent}>
      <WinChrome theme="atelier">
        Wotold <b>· Лонч в августе — Марина</b>
      </WinChrome>
      <div className="ate-body">
        <AteNav active="calls" />
        <div className="ate-main" style={{ overflow: 'auto' }}>
          {/* Back */}
          <button
            className="btn btn--quiet"
            style={{ padding: 0, marginBottom: 14, fontSize: 13 }}
          >
            ← Все звонки
          </button>

          {/* Header */}
          <div style={{ marginBottom: 22 }}>
            <div className="small-caps" style={{ marginBottom: 8 }}>
              Вторник · 19 мая · 11:24 · 32 мин 14 сек
            </div>
            <div className="title" style={{ fontSize: 36 }}>
              Лонч в августе — Марина
            </div>
            <div
              style={{
                display: 'flex',
                gap: 8,
                marginTop: 14,
                alignItems: 'center',
              }}
            >
              <AteSpeakerChip name="Айдар Жунусов" colorIdx={0} />
              <AteSpeakerChip name="Марина Сергеева" colorIdx={1} />
              <span className="muted" style={{ fontSize: 12, marginLeft: 8 }}>
                · 2 участника
              </span>
            </div>
          </div>

          {/* Tabs */}
          <div className="tabs">
            <button className="tab tab--active">Расшифровка</button>
            <button className="tab">Рекап</button>
            <button className="tab">Задачи · 4</button>
            <button className="tab">Участники</button>
          </div>

          {/* Transcript */}
          <div style={{ marginTop: 4 }}>
            {SAMPLE_TRANSCRIPT.map((row, i) => (
              <div key={i} className="transcript-row">
                <div
                  className="transcript-speaker"
                  style={{ color: SP_COLORS_A[row.sp] }}
                >
                  {row.name}
                </div>
                <div className="transcript-text">{row.text}</div>
                <div className="transcript-time">{row.t}</div>
              </div>
            ))}
            <div
              className="muted"
              style={{
                fontFamily: 'var(--serif)',
                fontStyle: 'italic',
                fontSize: 13,
                textAlign: 'center',
                padding: '20px 0',
              }}
            >
              · · ·
            </div>
          </div>

          {/* Floating scrubber */}
          <div
            style={{
              position: 'sticky',
              bottom: 12,
              left: 0,
              right: 0,
              background: 'var(--paper)',
              border: '1px solid var(--line)',
              borderRadius: 999,
              padding: '8px 14px 8px 8px',
              display: 'flex',
              alignItems: 'center',
              gap: 12,
              boxShadow: '0 6px 24px rgba(26,22,18,0.08)',
              marginTop: 16,
            }}
          >
            <div
              style={{
                width: 32,
                height: 32,
                borderRadius: '50%',
                background: 'var(--ink)',
                color: 'var(--paper)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontSize: 11,
              }}
            >
              ▶
            </div>
            <div
              className="mono"
              style={{ fontSize: 11, color: 'var(--muted)' }}
            >
              00:00:48
            </div>
            <div style={{ flex: 1, height: 18 }}>
              <Waveform
                seed={11}
                color="var(--accent)"
                width={500}
                height={18}
                count={140}
                gap={1}
                opacity={0.4}
              />
            </div>
            <div
              className="mono"
              style={{ fontSize: 11, color: 'var(--muted)' }}
            >
              32:14
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ============================================================
// 6. Call detail — Recap (dossier)
// ============================================================
function AteCallDetailRecap({ theme = 'light' } = {}) {
  const { accent = 'persian' } = React.useContext(window.AtelierContext || React.createContext({}));
  return (
    <div className="atelier win" data-theme={theme} data-accent={accent}>
      <WinChrome theme="atelier">
        Wotold <b>· Лонч в августе — Марина · Рекап</b>
      </WinChrome>
      <div className="ate-body">
        <AteNav active="calls" />
        <div className="ate-main" style={{ overflow: 'auto' }}>
          <button
            className="btn btn--quiet"
            style={{ padding: 0, marginBottom: 14, fontSize: 13 }}
          >
            ← Все звонки
          </button>

          <div style={{ marginBottom: 22 }}>
            <div className="small-caps" style={{ marginBottom: 8 }}>
              Вторник · 19 мая · 11:24 · 32 мин
            </div>
            <div className="title" style={{ fontSize: 36 }}>
              Лонч в августе — Марина
            </div>
          </div>

          <div className="tabs">
            <button className="tab">Расшифровка</button>
            <button className="tab tab--active">Рекап</button>
            <button className="tab">Задачи · 4</button>
            <button className="tab">Участники</button>
          </div>

          {/* Dossier columns */}
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: '1fr 280px',
              gap: 56,
              marginTop: 8,
            }}
          >
            {/* main column */}
            <div>
              <section style={{ marginBottom: 32 }}>
                <div className="small-caps" style={{ marginBottom: 8 }}>
                  Резюме
                </div>
                <p
                  style={{
                    fontFamily: 'var(--serif)',
                    fontSize: 19,
                    lineHeight: 1.55,
                    color: 'var(--ink)',
                    letterSpacing: '-0.005em',
                    margin: 0,
                  }}
                >
                  Решили{' '}
                  <strong style={{ color: 'var(--accent)' }}>
                    сдвинуть лонч на 12 августа
                  </strong>{' '}
                  — это даёт буфер на нотаризацию приложения и тесты диаризации на
                  наложениях речи. Марина соберёт прессу к пятнице, Дима подключает
                  Gladia как fallback провайдер.
                </p>
              </section>

              <section style={{ marginBottom: 32 }}>
                <div className="small-caps" style={{ marginBottom: 12 }}>
                  Ключевые моменты
                </div>
                <ol
                  style={{
                    fontFamily: 'var(--serif)',
                    fontSize: 16,
                    lineHeight: 1.6,
                    paddingLeft: 0,
                    listStyle: 'none',
                    margin: 0,
                  }}
                >
                  {[
                    'Soniox даёт лучшее качество, но путается на наложениях.',
                    'Gladia добавляем как fallback — переключение через настройки.',
                    'Сдвиг релиза на 12 августа согласован.',
                    'Пресс-релиз готовит Марина к пятнице.',
                  ].map((t, i) => (
                    <li
                      key={i}
                      style={{
                        display: 'flex',
                        gap: 14,
                        padding: '6px 0',
                        borderBottom: '1px dotted var(--line-soft)',
                      }}
                    >
                      <span
                        className="mono"
                        style={{
                          color: 'var(--accent)',
                          minWidth: 22,
                          letterSpacing: '0.04em',
                        }}
                      >
                        0{i + 1}
                      </span>
                      <span>{t}</span>
                    </li>
                  ))}
                </ol>
              </section>

              <section>
                <div className="small-caps" style={{ marginBottom: 12 }}>
                  Задачи · 4
                </div>
                {SAMPLE_TASKS.map((t, i) => (
                  <div
                    key={i}
                    style={{
                      display: 'flex',
                      alignItems: 'baseline',
                      gap: 12,
                      padding: '12px 0',
                      borderBottom: '1px dotted var(--line-soft)',
                    }}
                  >
                    <span
                      style={{
                        width: 16,
                        height: 16,
                        border: `1.5px solid ${
                          t.done ? 'var(--accent)' : 'var(--line)'
                        }`,
                        background: t.done ? 'var(--accent)' : 'transparent',
                        borderRadius: 3,
                        display: 'inline-flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        color: 'var(--paper)',
                        fontSize: 10,
                        flexShrink: 0,
                        position: 'relative',
                        top: 3,
                      }}
                    >
                      {t.done ? '✓' : ''}
                    </span>
                    <div style={{ flex: 1 }}>
                      <div
                        style={{
                          fontFamily: 'var(--serif)',
                          fontSize: 16,
                          color: 'var(--ink)',
                          textDecoration: t.done ? 'line-through' : 'none',
                          opacity: t.done ? 0.55 : 1,
                        }}
                      >
                        {t.text}
                      </div>
                    </div>
                    <AteSpeakerChip
                      name={t.owner}
                      colorIdx={
                        t.owner === 'Марина'
                          ? 1
                          : t.owner === 'Дима'
                          ? 2
                          : 3
                      }
                    />
                  </div>
                ))}
              </section>
            </div>

            {/* side column */}
            <aside>
              <div
                style={{
                  borderTop: '1px solid var(--line)',
                  borderBottom: '1px solid var(--line)',
                  padding: '14px 0',
                  marginBottom: 18,
                }}
              >
                <div className="small-caps" style={{ marginBottom: 10 }}>
                  Метаданные
                </div>
                <div
                  style={{
                    display: 'flex',
                    flexDirection: 'column',
                    gap: 8,
                    fontSize: 12,
                  }}
                >
                  {[
                    ['Дата', '19 мая 2026'],
                    ['Провайдер', 'Soniox · managed'],
                    ['Язык', 'ru-RU'],
                    ['Размер', '14.2 МБ'],
                    ['Слов', '4 312'],
                  ].map(([k, v]) => (
                    <div
                      key={k}
                      style={{
                        display: 'flex',
                        justifyContent: 'space-between',
                      }}
                    >
                      <span className="muted">{k}</span>
                      <span className="mono">{v}</span>
                    </div>
                  ))}
                </div>
              </div>

              <div className="small-caps" style={{ marginBottom: 12 }}>
                Участники
              </div>
              <div
                style={{ display: 'flex', flexDirection: 'column', gap: 10 }}
              >
                <AteSpeakerChip name="Айдар Жунусов" colorIdx={0} />
                <AteSpeakerChip name="Марина Сергеева" colorIdx={1} />
              </div>

              <button
                className="btn btn--ghost"
                style={{ marginTop: 20, width: '100%' }}
              >
                Экспорт в MD
              </button>
            </aside>
          </div>
        </div>
      </div>
    </div>
  );
}

// ============================================================
// 7. Speaker confirmation — calling-card modal
// ============================================================
function AteSpeakerConfirm({ theme = 'light' } = {}) {
  const { accent = 'persian' } = React.useContext(window.AtelierContext || React.createContext({}));
  return (
    <div
      className="atelier win"
      data-theme={theme}
      data-accent={accent}
      style={{
        background:
          'radial-gradient(ellipse at center, var(--bg) 0%, var(--bg-2) 100%)',
      }}
    >
      <WinChrome theme="atelier">
        Wotold <b>· Подтвердить голоса</b>
      </WinChrome>
      <div
        style={{
          flex: 1,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          padding: 30,
        }}
      >
        <div
          className="index-card"
          style={{ width: 560, position: 'relative' }}
        >
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'baseline',
              marginBottom: 6,
            }}
          >
            <div className="small-caps">Голос 2 из 3</div>
            <div className="small-caps muted">
              из звонка · Лонч в августе
            </div>
          </div>

          <div className="title" style={{ fontSize: 28, marginBottom: 28 }}>
            Кто этот голос?
          </div>

          {/* speaker bubble + wave */}
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 22,
              padding: '16px 0',
              borderTop: '1px solid var(--line-soft)',
              borderBottom: '1px solid var(--line-soft)',
              marginBottom: 22,
            }}
          >
            <div
              style={{
                width: 56,
                height: 56,
                borderRadius: '50%',
                background: SP_COLORS_A[1],
                color: 'var(--paper)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontFamily: 'var(--mono)',
                fontWeight: 600,
                fontSize: 16,
                letterSpacing: '0.04em',
              }}
            >
              S2
            </div>
            <div style={{ flex: 1 }}>
              <div
                style={{
                  fontFamily: 'var(--serif)',
                  fontStyle: 'italic',
                  fontSize: 16,
                  marginBottom: 8,
                  color: 'var(--ink)',
                  letterSpacing: '-0.01em',
                }}
              >
                «Тогда возьмём бэйкап. По датам — что предлагаешь?»
              </div>
              <div style={{ height: 22, width: '100%' }}>
                <Waveform
                  seed={91}
                  color={SP_COLORS_A[1]}
                  count={64}
                  gap={1.5}
                  width={400}
                  height={22}
                />
              </div>
            </div>
            <button
              className="btn btn--ghost"
              style={{ padding: '8px 12px', fontSize: 12 }}
            >
              ▶ 4 сек
            </button>
          </div>

          {/* Suggestion */}
          <div className="small-caps" style={{ marginBottom: 10 }}>
            Похоже на
          </div>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 14,
              marginBottom: 24,
            }}
          >
            <div
              className="sp-avatar"
              style={{
                background: SP_COLORS_A[1],
                width: 38,
                height: 38,
                fontSize: 12,
              }}
            >
              МС
            </div>
            <div style={{ flex: 1 }}>
              <div
                style={{
                  fontFamily: 'var(--serif)',
                  fontSize: 17,
                  letterSpacing: '-0.01em',
                }}
              >
                Марина Сергеева
              </div>
              <div className="muted" style={{ fontSize: 12 }}>
                Co-founder · НовоСтор · 14 предыдущих звонков
              </div>
            </div>
            <div style={{ width: 120 }}>
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  marginBottom: 4,
                  fontSize: 11,
                }}
              >
                <span className="small-caps">Уверенность</span>
                <span className="mono">92%</span>
              </div>
              <div className="conf">
                <div className="conf-fill" style={{ width: '92%' }} />
              </div>
            </div>
          </div>

          <div
            style={{
              display: 'flex',
              gap: 10,
              borderTop: '1px solid var(--line-soft)',
              paddingTop: 18,
            }}
          >
            <button
              className="btn btn--primary"
              style={{ flex: 1, justifyContent: 'center' }}
            >
              ✓ Да, это Марина
            </button>
            <button className="btn btn--ghost">Не она</button>
            <button className="btn btn--ghost">Новый контакт</button>
          </div>

          <div
            style={{
              marginTop: 14,
              textAlign: 'center',
              fontSize: 12,
            }}
          >
            <span className="muted">
              Подтверждение сохранит голос в профиль контакта —{' '}
            </span>
            <button
              className="btn btn--quiet"
              style={{ padding: 0, fontSize: 12, textDecoration: 'underline' }}
            >
              подробнее
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ============================================================
// 8. Contacts
// ============================================================
function AteContacts({ theme = 'light' } = {}) {
  const { accent = 'persian' } = React.useContext(window.AtelierContext || React.createContext({}));
  return (
    <div className="atelier win" data-theme={theme} data-accent={accent}>
      <WinChrome theme="atelier">
        Wotold <b>· Контакты</b>
      </WinChrome>
      <div className="ate-body">
        <AteNav active="contacts" />
        <div className="ate-main" style={{ padding: 0, display: 'flex' }}>
          {/* List */}
          <div
            style={{
              width: 320,
              borderRight: '1px solid var(--line-soft)',
              padding: '32px 24px',
              overflow: 'auto',
            }}
          >
            <div
              style={{
                display: 'flex',
                alignItems: 'baseline',
                justifyContent: 'space-between',
                marginBottom: 18,
              }}
            >
              <div className="title" style={{ fontSize: 24 }}>
                Контакты
              </div>
              <button
                className="btn btn--quiet"
                style={{ padding: 0, fontSize: 13 }}
              >
                +
              </button>
            </div>
            <input
              className="input"
              placeholder="Поиск…"
              style={{ marginBottom: 20, fontSize: 14 }}
            />

            <div
              className="small-caps"
              style={{ marginBottom: 10, marginTop: 12 }}
            >
              А — М
            </div>
            {SAMPLE_CONTACTS.slice(0, 4).map((c, i) => (
              <button
                key={c.id}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 12,
                  padding: '10px 8px',
                  width: '100%',
                  border: 'none',
                  background: i === 1 ? 'var(--bg-2)' : 'transparent',
                  borderRadius: 6,
                  textAlign: 'left',
                  marginBottom: 2,
                }}
              >
                <span
                  className="sp-avatar"
                  style={{
                    background: SP_COLORS_A[i % 5],
                    width: 30,
                    height: 30,
                    fontSize: 11,
                  }}
                >
                  {c.initials}
                </span>
                <div style={{ flex: 1 }}>
                  <div
                    style={{
                      fontFamily: 'var(--serif)',
                      fontSize: 15,
                      color: 'var(--ink)',
                      letterSpacing: '-0.01em',
                    }}
                  >
                    {c.name}
                  </div>
                  <div className="muted" style={{ fontSize: 11 }}>
                    {c.role}
                  </div>
                </div>
              </button>
            ))}
            <div
              className="small-caps"
              style={{ marginBottom: 10, marginTop: 18 }}
            >
              Н — Я
            </div>
            {SAMPLE_CONTACTS.slice(4).map((c, i) => (
              <button
                key={c.id}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 12,
                  padding: '10px 8px',
                  width: '100%',
                  border: 'none',
                  background: 'transparent',
                  borderRadius: 6,
                  textAlign: 'left',
                  marginBottom: 2,
                }}
              >
                <span
                  className="sp-avatar"
                  style={{
                    background: SP_COLORS_A[(i + 4) % 5],
                    width: 30,
                    height: 30,
                    fontSize: 11,
                  }}
                >
                  {c.initials}
                </span>
                <div style={{ flex: 1 }}>
                  <div
                    style={{
                      fontFamily: 'var(--serif)',
                      fontSize: 15,
                      color: 'var(--ink)',
                      letterSpacing: '-0.01em',
                    }}
                  >
                    {c.name}
                  </div>
                  <div className="muted" style={{ fontSize: 11 }}>
                    {c.role}
                  </div>
                </div>
              </button>
            ))}
          </div>

          {/* Detail */}
          <div
            style={{
              flex: 1,
              padding: '32px 44px',
              overflow: 'auto',
              background: 'var(--paper)',
            }}
          >
            <div className="small-caps" style={{ marginBottom: 12 }}>
              Контакт
            </div>
            <div
              style={{ display: 'flex', alignItems: 'center', gap: 22, marginBottom: 28 }}
            >
              <span
                className="sp-avatar"
                style={{
                  background: SP_COLORS_A[1],
                  width: 76,
                  height: 76,
                  fontSize: 22,
                }}
              >
                КА
              </span>
              <div>
                <div
                  className="display"
                  style={{ fontSize: 38, marginBottom: 6 }}
                >
                  Кенесары Абилов
                </div>
                <div
                  className="subtitle"
                  style={{ fontSize: 15, fontStyle: 'normal' }}
                >
                  CTO · НовоСтор
                </div>
              </div>
            </div>

            <div style={{ display: 'flex', gap: 18, marginBottom: 32 }}>
              <div className="stat" style={{ padding: '0 24px 0 0' }}>
                <span className="stat-value">6</span>
                <span className="stat-label">Звонков</span>
              </div>
              <div className="stat">
                <span className="stat-value">3<span style={{ fontSize: 18 }}>ч 4м</span></span>
                <span className="stat-label">Записано</span>
              </div>
              <div className="stat">
                <span className="stat-value">3</span>
                <span className="stat-label">Голосовых семпла</span>
              </div>
            </div>

            <div style={{ marginBottom: 28 }}>
              <div className="small-caps" style={{ marginBottom: 14 }}>
                Контакты
              </div>
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: '1fr 1fr',
                  gap: '14px 32px',
                }}
              >
                {[
                  ['Email', 'kenesary@novostor.kz'],
                  ['Телефон', '+7 701 245 18 03'],
                  ['Telegram', '@kenesary_a'],
                  ['Организация', 'НовоСтор LLP · Алматы'],
                ].map(([k, v]) => (
                  <div key={k}>
                    <div className="small-caps" style={{ marginBottom: 2 }}>
                      {k}
                    </div>
                    <div
                      style={{
                        fontFamily: 'var(--serif)',
                        fontSize: 15,
                        color: 'var(--ink)',
                      }}
                    >
                      {v}
                    </div>
                  </div>
                ))}
              </div>
            </div>

            <div style={{ marginBottom: 28 }}>
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'baseline',
                  marginBottom: 14,
                }}
              >
                <div className="small-caps">Голосовые семплы · 3</div>
                <span
                  className="muted"
                  style={{
                    fontFamily: 'var(--serif)',
                    fontStyle: 'italic',
                    fontSize: 13,
                  }}
                >
                  обновляются при подтверждении
                </span>
              </div>
              {[
                ['12 мая · Демо НовоСтор', '4.2 с', 92],
                ['08 мая · Бриф', '2.8 с', 88],
                ['02 мая · Знакомство', '3.4 с', 81],
              ].map(([src, len, q], i) => (
                <div
                  key={i}
                  style={{
                    display: 'grid',
                    gridTemplateColumns: '170px 1fr 60px 70px 24px',
                    gap: 16,
                    alignItems: 'center',
                    padding: '10px 0',
                    borderTop: i === 0 ? 'none' : '1px dotted var(--line)',
                  }}
                >
                  <div
                    style={{
                      fontFamily: 'var(--serif)',
                      fontSize: 14,
                      color: 'var(--ink)',
                    }}
                  >
                    {src}
                  </div>
                  <div style={{ height: 20 }}>
                    <MiniWave
                      seed={i + 30}
                      color={SP_COLORS_A[1]}
                      width={300}
                      height={20}
                      count={50}
                    />
                  </div>
                  <div
                    className="mono muted"
                    style={{ fontSize: 11, letterSpacing: '0.04em' }}
                  >
                    {len}
                  </div>
                  <div
                    className="mono"
                    style={{ fontSize: 11, color: 'var(--accent)' }}
                  >
                    {q}% качество
                  </div>
                  <button
                    className="btn btn--quiet"
                    style={{ padding: 0, fontSize: 14 }}
                  >
                    ×
                  </button>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ============================================================
// 9. Settings — BYO keys section
// ============================================================
function AteSettings({ theme = 'light' } = {}) {
  const { accent = 'persian' } = React.useContext(window.AtelierContext || React.createContext({}));
  return (
    <div className="atelier win" data-theme={theme} data-accent={accent}>
      <WinChrome theme="atelier">
        Wotold <b>· Настройки</b>
      </WinChrome>
      <div className="ate-body">
        <AteNav active="settings" />
        <div className="ate-main" style={{ padding: 0, display: 'flex' }}>
          {/* Settings rail */}
          <div
            style={{
              width: 220,
              padding: '32px 22px',
              borderRight: '1px solid var(--line-soft)',
            }}
          >
            <div className="small-caps" style={{ marginBottom: 14 }}>
              Настройки
            </div>
            {[
              ['acc', 'Учётная запись'],
              ['perm', 'Разрешения'],
              ['keys', 'Ключи (BYO)', true],
              ['voice', 'Голосовые семплы'],
              ['usage', 'Использование'],
              ['speakers', 'Спикеры'],
              ['advanced', 'Дополнительно'],
            ].map(([id, label, active]) => (
              <button
                key={id}
                className={`ate-nav-item${active ? ' ate-nav-item--active' : ''}`}
                style={{ fontSize: 14 }}
              >
                {label}
              </button>
            ))}
          </div>

          {/* Settings content */}
          <div style={{ flex: 1, padding: '32px 44px', overflow: 'auto' }}>
            <div className="small-caps" style={{ marginBottom: 8 }}>
              Настройки · Ключи
            </div>
            <div className="display" style={{ fontSize: 40, marginBottom: 10 }}>
              Свои ключи API.
            </div>
            <p
              className="subtitle"
              style={{ maxWidth: 520, marginBottom: 32 }}
            >
              По умолчанию Wotold ходит через наш прокси с дневной бесплатной квотой.
              Подключите свои ключи — и запросы пойдут напрямую, мимо нас, без лимитов.
            </p>

            {/* Path toggle */}
            <div
              style={{
                background: 'var(--paper)',
                border: '1px solid var(--line)',
                borderRadius: 8,
                padding: 18,
                marginBottom: 36,
                display: 'flex',
                alignItems: 'center',
                gap: 18,
              }}
            >
              <span className="small-caps">Путь</span>
              <div
                style={{
                  display: 'flex',
                  border: '1px solid var(--line)',
                  borderRadius: 999,
                  padding: 3,
                  background: 'var(--bg)',
                }}
              >
                <button
                  className="mono"
                  style={{
                    fontSize: 11,
                    padding: '6px 14px',
                    border: 'none',
                    borderRadius: 999,
                    background: 'var(--accent)',
                    color: 'var(--paper)',
                    letterSpacing: '0.12em',
                    textTransform: 'uppercase',
                    fontWeight: 600,
                  }}
                >
                  Свои ключи
                </button>
                <button
                  className="mono muted"
                  style={{
                    fontSize: 11,
                    padding: '6px 14px',
                    border: 'none',
                    borderRadius: 999,
                    background: 'transparent',
                    letterSpacing: '0.12em',
                    textTransform: 'uppercase',
                    fontWeight: 600,
                  }}
                >
                  Через прокси
                </button>
              </div>
              <span
                className="muted"
                style={{
                  fontFamily: 'var(--serif)',
                  fontStyle: 'italic',
                  fontSize: 13,
                  marginLeft: 'auto',
                }}
              >
                Ключи хранятся в системном Keychain
              </span>
            </div>

            {/* Key fields */}
            <div
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: 28,
              }}
            >
              {[
                {
                  label: 'Soniox · STT',
                  value: 'sk_live_••••••••••••••••s9Kd',
                  on: true,
                  hint: 'Распознавание + диаризация · primary',
                },
                {
                  label: 'Gladia · STT',
                  value: 'gl_••••••••••••••••3aPx',
                  on: true,
                  hint: 'Fallback при ошибке Soniox',
                },
                {
                  label: 'Anthropic · LLM',
                  value: 'sk-ant-•••••••••••••••••a1c',
                  on: true,
                  hint: 'Рекапы, МоМ, подсказки спикеров',
                },
              ].map((k) => (
                <div key={k.label} className="field">
                  <div
                    style={{
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'baseline',
                      marginBottom: 4,
                    }}
                  >
                    <label className="field-label">{k.label}</label>
                    <span
                      style={{
                        fontFamily: 'var(--mono)',
                        fontSize: 10.5,
                        color: 'var(--accent)',
                        letterSpacing: '0.14em',
                        textTransform: 'uppercase',
                      }}
                    >
                      ● подключён
                    </span>
                  </div>
                  <input className="input" defaultValue={k.value} />
                  <span
                    className="muted"
                    style={{
                      fontFamily: 'var(--serif)',
                      fontStyle: 'italic',
                      fontSize: 13,
                      marginTop: 4,
                    }}
                  >
                    {k.hint}
                  </span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  AteCallsList, AteCallDetailTranscript, AteCallDetailRecap,
  AteSpeakerConfirm, AteContacts, AteSettings,
});
