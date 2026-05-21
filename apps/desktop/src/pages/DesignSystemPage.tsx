// [B17] DesignSystemPage — exact match per docs/design/atelier-v2/_reference/
// design-system.jsx. Single-scroll showcase секций (DSCard/DSSectionTitle/
// ColorSwatch/TypeRow extracted в DesignSystemBits.tsx).

import type { CSSProperties } from 'react';
import { Waveform } from '../components/Waveform';
import {
  CallErrorRow,
  CallStateTag,
  PipelineStrip,
  ProgressRail,
} from '../components/call-state';
import type { CallState } from '../types/callState';
import {
  ColorSwatch,
  DSCard,
  DSSectionTitle,
  TypeRow,
} from './DesignSystemBits';

const SECTION_GAP: CSSProperties = { marginBottom: 38 };

export function DesignSystemPage() {
  return (
    <section>
      {/* Hero */}
      <div className="eyebrow" style={{ marginBottom: 14 }}>
        Wotold Atelier · Design tokens · dev only
      </div>
      <div className="display" style={{ marginBottom: 14 }}>
        Design tokens<span style={{ color: 'var(--accent)' }}>.</span>
      </div>
      <p className="subtitle" style={{ maxWidth: 640, marginBottom: 44 }}>
        Все цвета, шрифты и компоненты для редизайна Wotold. Переключай тему
        и акцент в Настройках — все экраны (и эта страница) реагируют
        одновременно.
      </p>

      {/* 01 Surface & ink */}
      <DSSectionTitle
        eyebrow="01 · Colors"
        title="Surface & ink"
        subtitle="Cool neutral paper. Никаких тёплых кремов, никакой terracotta — это была проблема первой итерации."
      />
      <DSCard style={SECTION_GAP}>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(4, 1fr)',
            gap: 16,
          }}
        >
          <ColorSwatch token="bg" hex="#F4F4F2" fgVar sub="page background" />
          <ColorSwatch token="bg-2" hex="#ECECE8" fgVar sub="hover / inset" />
          <ColorSwatch token="paper" hex="#FCFCFA" fgVar sub="rail / chrome" />
          <ColorSwatch
            token="surface"
            hex="#FFFFFF"
            fgVar
            sub="cards / inputs"
          />
          <ColorSwatch token="line" hex="#E1E0DA" fgVar sub="dividers" />
          <ColorSwatch
            token="line-soft"
            hex="#ECEAE3"
            fgVar
            sub="soft dividers"
          />
          <ColorSwatch
            token="line-strong"
            hex="#C8C7C0"
            fgVar
            sub="input borders"
          />
          <ColorSwatch token="ink" hex="#14151A" sub="primary text" />
          <ColorSwatch token="ink-2" hex="#2A2B30" sub="secondary text" />
          <ColorSwatch token="muted" hex="#6B6C72" sub="muted text" />
          <ColorSwatch token="subtle" hex="#9C9D9F" sub="subtle text" />
        </div>
      </DSCard>

      {/* 01a Accent · Signal */}
      <DSSectionTitle
        eyebrow="01a · Accent · Signal"
        title="Bordeaux (по умолчанию)"
        subtitle="Акцент — для primary actions, focus, индикатора активной вкладки. Red (signal) зарезервирован под запись/danger — это единственное использование."
      />
      <DSCard style={SECTION_GAP}>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(4, 1fr)',
            gap: 16,
          }}
        >
          <ColorSwatch token="accent" hex="#7E1F2A" sub="primary actions" />
          <ColorSwatch token="accent-hover" hex="#8E2536" sub="hover" />
          <ColorSwatch
            token="accent-soft"
            hex="#F2DCDF"
            fgVar
            sub="tints, focus rings"
          />
          <ColorSwatch
            token="accent-fg"
            hex="#FFFFFF"
            fgVar
            sub="text on accent"
          />
          <ColorSwatch
            token="signal"
            hex="#DC2626"
            sub="record · danger ONLY"
          />
          <ColorSwatch
            token="signal-soft"
            hex="#FCE7E7"
            fgVar
            sub="record button halo"
          />
        </div>
      </DSCard>

      {/* 01b Speaker palette */}
      <DSSectionTitle
        eyebrow="01b · Speaker palette"
        title="Голоса как цвета"
        subtitle="5 цветов на 5 спикеров. Используются как фон аватара, dot, бордюр на активной строке. Различимы и на светлой, и на тёмной теме."
      />
      <DSCard style={SECTION_GAP}>
        <div style={{ display: 'flex', gap: 12, flexWrap: 'wrap' }}>
          {(
            [
              ['sp-1', '#3D5BAB'],
              ['sp-2', '#2E8C5F'],
              ['sp-3', '#B86842'],
              ['sp-4', '#7958C7'],
              ['sp-5', '#3D87A4'],
            ] as Array<[string, string]>
          ).map(([token, hex], i) => (
            <div
              key={token}
              style={{ display: 'flex', alignItems: 'center', gap: 12 }}
            >
              <span
                className="sp-avatar"
                style={{
                  background: `var(--${token})`,
                  width: 40,
                  height: 40,
                  fontSize: 12,
                }}
              >
                S{i + 1}
              </span>
              <div>
                <div
                  className="mono"
                  style={{ fontSize: 11, color: 'var(--ink)' }}
                >
                  --{token}
                </div>
                <div className="muted" style={{ fontSize: 11 }}>
                  {hex}
                </div>
              </div>
            </div>
          ))}
        </div>
      </DSCard>

      {/* 02 Typography */}
      <DSSectionTitle
        eyebrow="02 · Typography"
        title="Source Serif 4 + DM Sans + JetBrains Mono"
        subtitle="Сериф — для контента (заголовки, расшифровка, имена). DM Sans — для UI. Mono — для времени, ID, метаданных."
      />
      <DSCard style={SECTION_GAP}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 24 }}>
          <TypeRow
            label="Display · Source Serif 4 400"
            size="54px / 1.05 / -0.028em"
            sample="Готов записывать."
            fam="var(--font-serif)"
            s={54}
            w={400}
            ls="-0.028em"
            lh={1.05}
          />
          <TypeRow
            label="Title · Source Serif 4 500"
            size="28px / 1.15 / -0.025em"
            sample="Лонч в августе — Марина"
            fam="var(--font-serif)"
            s={28}
            w={500}
            ls="-0.025em"
            lh={1.15}
          />
          <TypeRow
            label="Subtitle · Source Serif 4 400"
            size="17px / 1.5 / -0.005em"
            sample="Wotold отделяет вашу речь от речи собеседника."
            fam="var(--font-serif)"
            s={17}
            w={400}
            ls="-0.005em"
            lh={1.5}
          />
          <TypeRow
            label="Transcript · Source Serif 4 400"
            size="17px / 1.55 / -0.008em"
            sample="«Тогда возьмём бэйкап. По датам — что предлагаешь?»"
            fam="var(--font-serif)"
            s={17}
            w={400}
            ls="-0.008em"
            lh={1.55}
            italic
          />
          <TypeRow
            label="Body · DM Sans 400"
            size="14px / 1.45 / -0.005em"
            sample="Стандартный UI-текст для интерфейса и описаний."
            fam="var(--font-sans)"
            s={14}
            w={400}
            ls="-0.005em"
            lh={1.45}
          />
          <TypeRow
            label="Eyebrow · DM Sans 600 caps"
            size="11px / 1 / 0.14em"
            sample="ПОДТВЕРДИТЬ ГОЛОС · 02/03"
            fam="var(--font-sans)"
            s={11}
            w={600}
            ls="0.14em"
            lh={1}
            upper
          />
          <TypeRow
            label="Mono · JetBrains Mono 500"
            size="11px / 1 / 0.04em"
            sample="00:14:23 · sk_live_••••s9Kd"
            fam="var(--font-mono)"
            s={11}
            w={500}
            ls="0.04em"
            lh={1}
          />
        </div>
      </DSCard>

      {/* 03 Radii + Shadows */}
      <DSSectionTitle
        eyebrow="03 · Radii · Shadows"
        title="Скругления и тени"
        subtitle="4 радиуса, 3 уровня высоты."
      />
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: '1fr 1fr',
          gap: 18,
          marginBottom: 38,
        }}
      >
        <DSCard>
          <div className="small-caps" style={{ marginBottom: 14 }}>
            Radii
          </div>
          <div style={{ display: 'flex', gap: 14, flexWrap: 'wrap' }}>
            {(
              [
                ['sm', 4],
                ['md', 6],
                ['lg', 10],
                ['xl', 16],
                ['pill', 999],
              ] as Array<[string, number]>
            ).map(([name, r]) => (
              <div
                key={name}
                style={{
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 6,
                  alignItems: 'center',
                }}
              >
                <div
                  style={{
                    width: 60,
                    height: 60,
                    background: 'var(--accent)',
                    borderRadius: r,
                  }}
                />
                <div
                  className="mono"
                  style={{ fontSize: 10.5, color: 'var(--ink)' }}
                >
                  --radius-{name}
                </div>
                <div className="muted" style={{ fontSize: 10.5 }}>
                  {r < 999 ? `${r}px` : 'pill'}
                </div>
              </div>
            ))}
          </div>
        </DSCard>
        <DSCard>
          <div className="small-caps" style={{ marginBottom: 14 }}>
            Shadows
          </div>
          <div style={{ display: 'flex', gap: 18, flexWrap: 'wrap' }}>
            {([1, 2, 3] as const).map((n, i) => (
              <div
                key={n}
                style={{
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 6,
                  alignItems: 'center',
                }}
              >
                <div
                  style={{
                    width: 80,
                    height: 60,
                    background: 'var(--surface)',
                    borderRadius: 8,
                    boxShadow: `var(--shadow-${n})`,
                    border: '1px solid var(--line)',
                  }}
                />
                <div
                  className="mono"
                  style={{ fontSize: 10.5, color: 'var(--ink)' }}
                >
                  --shadow-{n}
                </div>
                <div className="muted" style={{ fontSize: 10.5 }}>
                  {['flat', 'raised', 'overlay'][i]}
                </div>
              </div>
            ))}
          </div>
        </DSCard>
      </div>

      {/* 04 Buttons */}
      <DSSectionTitle
        eyebrow="04 · Components"
        title="Buttons"
        subtitle="Primary · Ghost · Quiet · Danger. Sizes sm/md/lg. Record — отдельный «signature» компонент."
      />
      <DSCard style={SECTION_GAP}>
        <div
          style={{
            display: 'flex',
            gap: 14,
            flexWrap: 'wrap',
            marginBottom: 18,
          }}
        >
          <button type="button" className="btn btn--primary">
            Primary
          </button>
          <button type="button" className="btn btn--ghost">
            Ghost
          </button>
          <button type="button" className="btn btn--quiet">
            Quiet
          </button>
          <button type="button" className="btn btn--danger">
            Danger
          </button>
          <button
            type="button"
            className="btn btn--primary"
            disabled
            style={{ opacity: 0.5 }}
          >
            Disabled
          </button>
        </div>
        <div
          style={{
            display: 'flex',
            gap: 14,
            alignItems: 'center',
            flexWrap: 'wrap',
          }}
        >
          <button
            type="button"
            className="btn btn--primary"
            style={{ padding: '6px 12px', fontSize: 12.5 }}
          >
            Small
          </button>
          <button type="button" className="btn btn--primary">
            Medium
          </button>
          <button
            type="button"
            className="btn btn--primary"
            style={{ padding: '12px 20px', fontSize: 15 }}
          >
            Large
          </button>
          <span style={{ flex: 1 }} />
          <div style={{ display: 'flex', gap: 16, alignItems: 'center' }}>
            <button
              type="button"
              className="rec-btn"
              style={{ width: 72, height: 72 }}
              aria-label="record"
            />
            <div>
              <div className="small-caps" style={{ marginBottom: 2 }}>
                Record
              </div>
              <div className="mono muted" style={{ fontSize: 11 }}>
                signature button
              </div>
            </div>
          </div>
        </div>
      </DSCard>

      {/* 05 Chips · Status */}
      <DSSectionTitle
        eyebrow="05 · Chips · Status"
        title="Speaker chips, dots, badges"
      />
      <DSCard style={SECTION_GAP}>
        <div
          style={{
            display: 'flex',
            gap: 10,
            flexWrap: 'wrap',
            marginBottom: 18,
          }}
        >
          {(
            [
              ['ВЫ', 'Айдар Жунусов', 1],
              ['МС', 'Марина Сергеева', 2],
              ['КА', 'Кенесары', 3],
            ] as Array<[string, string, number]>
          ).map(([initials, name, n]) => (
            <span className="sp" key={name}>
              <span
                className="sp-avatar"
                style={{ background: `var(--sp-${n})` }}
              >
                {initials}
              </span>
              {name}
            </span>
          ))}
        </div>
        <div
          style={{
            display: 'flex',
            gap: 14,
            alignItems: 'center',
            flexWrap: 'wrap',
          }}
        >
          {(
            [
              ['accent', 'var(--accent)', false],
              ['signal · pulse', 'var(--signal)', true],
              ['success', 'var(--success)', false],
              ['warning', 'var(--warning)', false],
            ] as Array<[string, string, boolean]>
          ).map(([label, color, pulse]) => (
            <span
              key={label}
              style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}
            >
              <span
                className={`dot${pulse ? ' dot--pulse' : ''}`}
                style={{ background: color }}
              />
              <span className="small-caps">{label}</span>
            </span>
          ))}
        </div>
      </DSCard>

      {/* 06 Inputs */}
      <DSSectionTitle
        eyebrow="06 · Inputs"
        title="Поля ввода"
        subtitle="Подчёркнутые — для онбординга и форм. Boxed — для настроек / API ключей."
      />
      <DSCard style={SECTION_GAP}>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: '1fr 1fr',
            gap: '20px 32px',
          }}
        >
          <div className="field">
            <label className="field-label">Имя</label>
            <input className="input" defaultValue="Айдар Жунусов" />
          </div>
          <div className="field">
            <label className="field-label">Роль</label>
            <input className="input" placeholder="placeholder italic" />
          </div>
          <div className="field" style={{ gridColumn: '1 / -1' }}>
            <label className="field-label">API key (boxed)</label>
            <input
              className="input input--box"
              defaultValue="sk_live_••••••••••••••••s9Kd"
            />
          </div>
        </div>
      </DSCard>

      {/* 07 Tabs */}
      <DSSectionTitle
        eyebrow="07 · Tabs"
        title="Вкладки внутри страницы"
      />
      <DSCard style={SECTION_GAP}>
        <div className="tabs" style={{ marginBottom: 0 }}>
          <button type="button" className="tab tab--active">
            Расшифровка
          </button>
          <button type="button" className="tab">
            Рекап
          </button>
          <button type="button" className="tab">
            Задачи · 4
          </button>
          <button type="button" className="tab">
            Участники
          </button>
        </div>
      </DSCard>

      {/* 08 Transcript */}
      <DSSectionTitle
        eyebrow="08 · Transcript"
        title="Реплика"
        subtitle="Основной примитив самого важного экрана."
      />
      <DSCard style={SECTION_GAP}>
        <div className="transcript-row">
          <div className="transcript-speaker" style={{ color: 'var(--sp-1)' }}>
            ВЫ
          </div>
          <div className="transcript-text">
            Хочу обсудить лонч. Мы немного задерживаемся с диаризацией —
            Soniox даёт хорошее качество, но на наложениях речи путает спикеров.
          </div>
          <div className="transcript-time">00:00:12</div>
        </div>
        <div className="transcript-row">
          <div className="transcript-speaker" style={{ color: 'var(--sp-2)' }}>
            МАРИНА
          </div>
          <div className="transcript-text">
            Понимаю. А Gladia пробовали как fallback?
          </div>
          <div className="transcript-time">00:00:24</div>
        </div>
      </DSCard>

      {/* 09 Stats · Confidence */}
      <DSSectionTitle
        eyebrow="09 · Stats · Confidence"
        title="Числа и прогресс"
      />
      <DSCard style={SECTION_GAP}>
        <div style={{ display: 'flex', marginBottom: 28 }}>
          <div className="stat">
            <span className="stat-value">94</span>
            <span className="stat-label">Звонков · всего</span>
          </div>
          <div className="stat">
            <span className="stat-value">12</span>
            <span className="stat-label">За неделю</span>
          </div>
          <div className="stat">
            <span className="stat-value">
              38<span style={{ fontSize: 18, marginLeft: 4 }}>ч</span>
            </span>
            <span className="stat-label">В архиве</span>
          </div>
          <div className="stat">
            <span className="stat-value" style={{ color: 'var(--accent)' }}>
              3
            </span>
            <span className="stat-label">Ждут подтверждения</span>
          </div>
        </div>
        <div style={{ maxWidth: 360 }}>
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              marginBottom: 6,
            }}
          >
            <span className="small-caps">Уверенность</span>
            <span
              className="mono"
              style={{ fontSize: 11, color: 'var(--ink)' }}
            >
              92%
            </span>
          </div>
          <div className="conf">
            <div className="conf-fill" style={{ width: '92%' }} />
          </div>
        </div>
      </DSCard>

      {/* 10 Waveform */}
      <DSSectionTitle
        eyebrow="10 · Waveform"
        title="Звуковая дорожка"
        subtitle="Используется в записи и в скрабере звонка."
      />
      <DSCard style={SECTION_GAP}>
        <div style={{ marginBottom: 12 }}>
          <div className="small-caps" style={{ marginBottom: 6 }}>
            Вы · микрофон
          </div>
          <div style={{ height: 64 }}>
            <Waveform
              seed={42}
              color="var(--ink)"
              count={140}
              gap={2.5}
              width={900}
              height={64}
            />
          </div>
        </div>
        <div>
          <div
            className="small-caps"
            style={{ marginBottom: 6, color: 'var(--accent)' }}
          >
            Собеседник · системный звук
          </div>
          <div style={{ height: 64 }}>
            <Waveform
              seed={73}
              color="var(--accent)"
              count={140}
              gap={2.5}
              width={900}
              height={64}
            />
          </div>
        </div>
      </DSCard>

      {/* 14 · Async-state components (V6) */}
      <DSSectionTitle
        eyebrow="14 · Async states (V6)"
        title="Call lifecycle — tags, rails, pipeline, errors"
        subtitle="Шесть состояний звонка (live · uploading · queued · processing · ready · error) + рельсы прогресса + 5-step pipeline strip + quiet inline error. Всё реактивно к prefers-reduced-motion."
      />
      <DSCard style={SECTION_GAP}>
        {/* Stat tags — все 6 вариантов */}
        <div
          className="small-caps"
          style={{ marginBottom: 10, color: 'var(--text-muted)' }}
        >
          stat-tag · 6 variants
        </div>
        <div
          style={{
            display: 'flex',
            gap: 10,
            flexWrap: 'wrap',
            marginBottom: 22,
          }}
        >
          {(
            ['live', 'uploading', 'queued', 'processing', 'ready', 'error'] as CallState[]
          ).map((s) => (
            <CallStateTag key={s} state={s} />
          ))}
          <CallStateTag state="processing" detail="64%" />
          <CallStateTag state="uploading" detail="2.4 MB" />
        </div>

        {/* Progress rails */}
        <div
          className="small-caps"
          style={{ marginBottom: 10, color: 'var(--text-muted)' }}
        >
          rail · determinate / indeterminate
        </div>
        <div style={{ marginBottom: 8 }}>
          <ProgressRail pct={32} ariaLabel="32% complete" />
        </div>
        <div style={{ marginBottom: 8 }}>
          <ProgressRail pct={72} ariaLabel="72% complete" />
        </div>
        <div style={{ marginBottom: 22 }}>
          <ProgressRail indeterminate ariaLabel="Processing" />
        </div>

        {/* Pipeline strip — single open snapshot */}
        <div
          className="small-caps"
          style={{ marginBottom: 10, color: 'var(--text-muted)' }}
        >
          pipeline strip · expanded (step 3 / 5 · indeterminate rail)
        </div>
        <div style={{ marginBottom: 22 }}>
          <PipelineStrip
            progress={{
              step: 3,
              pct: 64,
              stageLabel: 'Распознаём речь',
              etaSec: 25,
            }}
            defaultOpen
          />
        </div>

        {/* Inline error row — как в CallsPage row */}
        <div
          className="small-caps"
          style={{ marginBottom: 10, color: 'var(--text-muted)' }}
        >
          call-error-row · quiet inline (calls list)
        </div>
        <div style={{ marginBottom: 22 }}>
          <CallErrorRow
            error={{
              code: 'STT_TIMEOUT',
              message: 'Сеть недоступна — попробуй ещё раз',
              attempts: 2,
              quotaConsumed: false,
            }}
            onOpenDetails={() => {
              /* DS demo — no-op */
            }}
          />
        </div>

        {/* Activity strip */}
        <div
          className="small-caps"
          style={{ marginBottom: 10, color: 'var(--text-muted)' }}
        >
          activity-strip · global processing indicator
        </div>
        <div
          className="activity-strip"
          style={{ marginBottom: 22 }}
        >
          <span className="stat-tag-dot" aria-hidden="true" />
          <span>Обрабатываем 2 звонка · можно закрыть окно</span>
        </div>

        {/* Ghost rows — transcript placeholder */}
        <div
          className="small-caps"
          style={{ marginBottom: 10, color: 'var(--text-muted)' }}
        >
          transcript-row--ghost · streaming placeholder
        </div>
        <div className="transcript">
          {[0, 1, 2].map((i) => (
            <div
              key={i}
              className="transcript-row transcript-row--ghost"
            >
              <div className="transcript-speaker" aria-hidden="true">
                ···
              </div>
              <div className="transcript-text" aria-hidden="true">
                ···
              </div>
              <div className="transcript-time" aria-hidden="true">
                ···
              </div>
            </div>
          ))}
        </div>
      </DSCard>

      {/* Footer */}
      <div
        className="mono muted"
        style={{
          fontSize: 11,
          textAlign: 'center',
          paddingTop: 24,
          borderTop: '1px solid var(--line-soft)',
          letterSpacing: '0.04em',
        }}
      >
        Wotold Atelier v2 · all tokens live as CSS variables · switch [data-theme] and [data-accent] on any root
      </div>
    </section>
  );
}
