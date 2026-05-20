/* eslint-disable */
// ─────────────────────────────────────────────────────────────
// Shared helpers — waveform SVGs, dummy data, window chrome
// ─────────────────────────────────────────────────────────────

// Deterministic pseudo-wave generator. Returns an SVG path string.
// width × height define viewport; the waveform is centered vertically.
function wavePath(seed, count, width, height, amp = 0.9, smoothing = false) {
  // tiny seeded RNG
  let s = seed >>> 0;
  const rand = () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return (s & 0xFFFF) / 0xFFFF;
  };
  const mid = height / 2;
  const step = width / count;
  const bars = [];
  for (let i = 0; i < count; i++) {
    // gaussian-ish modulation so it doesn't look uniform
    const env =
      0.35 +
      0.65 *
        Math.abs(
          Math.sin((i / count) * Math.PI * 2.7) * Math.cos((i / count) * Math.PI * 1.3)
        );
    const r = (rand() * 2 - 1) * amp * env;
    const h = Math.max(2, Math.abs(r) * mid);
    bars.push({ x: i * step + step / 2, h });
  }
  return bars;
}

function Waveform({ seed = 1, color = '#fff', height = 80, width = 800, count = 120, gap = 1.5, opacity = 1, style = {} }) {
  const bars = wavePath(seed, count, width, height);
  const barW = Math.max(1, width / count - gap);
  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      style={{ width: '100%', height: '100%', display: 'block', opacity, ...style }}
    >
      {bars.map((b, i) => (
        <rect
          key={i}
          x={b.x - barW / 2}
          y={height / 2 - b.h}
          width={barW}
          height={b.h * 2}
          rx={Math.min(barW / 2, 1.5)}
          fill={color}
        />
      ))}
    </svg>
  );
}

// Mini-wave for in-line transcript snippets
function MiniWave({ seed = 4, color = '#000', width = 100, height = 18, count = 28 }) {
  return <Waveform seed={seed} color={color} width={width} height={height} count={count} gap={1} />;
}

// Generic window chrome (cross-platform — no traffic-light mimicry)
function WinChrome({ children, theme = 'atelier' }) {
  if (theme === 'atelier') {
    return (
      <div className="ate-chrome">
        <div className="ate-chrome-dots">
          <span className="ate-chrome-dot" />
          <span className="ate-chrome-dot" />
          <span className="ate-chrome-dot" />
        </div>
        <div className="ate-chrome-title">{children}</div>
      </div>
    );
  }
  return (
    <div className="con-chrome">
      <span className="con-chrome-dot" />
      <span className="con-chrome-dot" />
      <span className="con-chrome-dot" />
      <span style={{ marginLeft: 8 }}>{children}</span>
    </div>
  );
}

// Russian sample data — used by both directions
const SAMPLE_CALLS = [
  { id: 'c1', title: 'Лонч в августе — Марина', when: 'Сегодня · 11:24', dur: '32:14', speakers: ['Вы', 'Марина'], status: 'ready', preview: 'Обсудили перенос диаризации, согласовали бэйкап через Gladia.' },
  { id: 'c2', title: 'Демо Wotold — НовоСтор', when: 'Вчера · 16:02', dur: '47:08', speakers: ['Вы', 'Кенесары', 'Алия'], status: 'ready', preview: 'Кенесары попросил MCP-интеграцию для своего стека Notion.' },
  { id: 'c3', title: 'Стандап четверг', when: '15 мая · 10:00', dur: '12:33', speakers: ['Вы', 'Дима', 'Майкл'], status: 'ready', preview: 'Майкл закрыл M11.5; Дима блочит R2-стейджинг.' },
  { id: 'c4', title: 'Инвестор: Эрик П.', when: '13 мая · 14:30', dur: '54:01', speakers: ['Вы', 'Эрик'], status: 'ready', preview: 'Эрик готов вложить $80k под Free → Paid конверсию.' },
  { id: 'c5', title: 'Ретро Q2 — команда', when: '10 мая · 17:00', dur: '01:08:22', speakers: ['Вы', 'Марина', 'Дима', 'Майкл', 'Алия'], status: 'ready', preview: 'Главный вывод — диаризация это то, ради чего юзер платит.' },
  { id: 'c6', title: 'Дочерний бриф — Алия', when: '8 мая · 09:48', dur: '18:55', speakers: ['Вы', 'Алия'], status: 'processing', preview: 'Идёт распознавание…' },
];

const SAMPLE_CONTACTS = [
  { id: 'p1', name: 'Марина Сергеева', role: 'Co-founder · НовоСтор', initials: 'МС', calls: 14, minutes: 312 },
  { id: 'p2', name: 'Кенесары Абилов', role: 'CTO · НовоСтор', initials: 'КА', calls: 6, minutes: 184 },
  { id: 'p3', name: 'Алия Жармагамбетова', role: 'Designer · Wotold', initials: 'АЖ', calls: 22, minutes: 543 },
  { id: 'p4', name: 'Дима Тапалов', role: 'Rust · Wotold', initials: 'ДТ', calls: 31, minutes: 812 },
  { id: 'p5', name: 'Майкл Чен', role: 'Infra · Wotold', initials: 'МЧ', calls: 18, minutes: 421 },
  { id: 'p6', name: 'Эрик Палтиэль', role: 'Angel investor', initials: 'ЭП', calls: 3, minutes: 156 },
];

// Diarized transcript snippet (call 1 — Марина)
const SAMPLE_TRANSCRIPT = [
  { sp: 0, name: 'Вы',     t: '00:00:04', text: 'Привет, Марина. Спасибо, что нашла время — постарался уложиться в 30 минут.' },
  { sp: 1, name: 'Марина', t: '00:00:09', text: 'Привет. Без проблем, я слушаю.' },
  { sp: 0, name: 'Вы',     t: '00:00:12', text: 'Хочу обсудить лонч. Мы немного задерживаемся с диаризацией — Soniox даёт хорошее качество, но на наложениях речи путает спикеров.' },
  { sp: 1, name: 'Марина', t: '00:00:24', text: 'Понимаю. А Gladia пробовали как fallback?' },
  { sp: 0, name: 'Вы',     t: '00:00:28', text: 'Да, провайдеры за единым интерфейсом — переключение в один настройке.' },
  { sp: 1, name: 'Марина', t: '00:00:34', text: 'Тогда возьмём бэйкап. По датам — что предлагаешь?' },
  { sp: 0, name: 'Вы',     t: '00:00:41', text: 'Сдвиг на 12 августа — это даёт буфер на нотаризацию и тест дикторов.' },
  { sp: 1, name: 'Марина', t: '00:00:48', text: 'Согласна. Я согласую с командой и пришлю пресс-релиз в пятницу.' },
];

// Tasks (recap output)
const SAMPLE_TASKS = [
  { text: 'Подтвердить дату лонча 12 августа с командой', owner: 'Марина', done: false },
  { text: 'Подключить Gladia как fallback провайдер', owner: 'Дима', done: true },
  { text: 'Подготовить пресс-релиз', owner: 'Марина', done: false },
  { text: 'Провести тест дикторов на наложениях', owner: 'Алия', done: false },
];

Object.assign(window, {
  Waveform, MiniWave, WinChrome,
  SAMPLE_CALLS, SAMPLE_CONTACTS, SAMPLE_TRANSCRIPT, SAMPLE_TASKS,
  AtelierContext: React.createContext({ accent: 'persian' }),
});
