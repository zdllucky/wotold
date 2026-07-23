// [B27.8] Парсер «[N]» / «[N, M]»-ссылок в тексте ответа LLM → сегменты для
// интерактивного рендера. Матч валиден только когда ВСЕ номера попадают в
// [1..fragmentCount] — прочие скобки (цитаты, даты, markdown) остаются текстом.

export type AnswerSegment =
  | { kind: 'text'; text: string }
  | { kind: 'refs'; indices: number[]; raw: string };

const REFS_RE = /\[(\d{1,3}(?:\s*,\s*\d{1,3})*)\]/g;

export function parseFragmentRefs(text: string, fragmentCount: number): AnswerSegment[] {
  if (!text || fragmentCount <= 0) {
    return text ? [{ kind: 'text', text }] : [];
  }
  const out: AnswerSegment[] = [];
  let last = 0;
  REFS_RE.lastIndex = 0;
  for (let m = REFS_RE.exec(text); m != null; m = REFS_RE.exec(text)) {
    const indices = m[1].split(',').map((s) => parseInt(s.trim(), 10));
    if (!indices.every((n) => n >= 1 && n <= fragmentCount)) continue; // не ссылка — текст
    if (m.index > last) out.push({ kind: 'text', text: text.slice(last, m.index) });
    out.push({ kind: 'refs', indices, raw: m[0] });
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push({ kind: 'text', text: text.slice(last) });
  return out;
}
