// [B26.10] Лёгкий fuzzy-матчер: субпоследовательность со скорингом.
// Без зависимостей (fuse.js не тащим — стиль проекта). Бонусы: начало
// слова, подряд идущие символы; лёгкий штраф за длину цели. null = не матч.

function normalize(s: string): string {
  return s.toLowerCase().replace(/ё/g, 'е');
}

const WORD_CHAR = /[\p{L}\p{N}]/u;

/** Скор совпадения query↔target; null — query не субпоследовательность. */
export function fuzzyScore(query: string, target: string): number | null {
  const q = normalize(query);
  const t = normalize(target);
  if (q.length === 0) return 0;

  let qi = 0;
  let score = 0;
  let streak = 0;
  for (let ti = 0; ti < t.length && qi < q.length; ti += 1) {
    if (t[ti] === q[qi]) {
      qi += 1;
      streak += 1;
      score += 1 + streak; // подряд — нарастающий бонус
      const prev = t[ti - 1];
      if (ti === 0 || (prev !== undefined && !WORD_CHAR.test(prev))) {
        score += 8; // начало слова
      }
    } else {
      streak = 0;
    }
  }
  if (qi < q.length) return null;
  return score - t.length * 0.05;
}

/** Отфильтрованный и отсортированный по score список. Пустой запрос — все. */
export function fuzzyFilter<T>(items: T[], query: string, key: (item: T) => string): T[] {
  if (normalize(query).length === 0) return items;
  return items
    .map((item) => ({ item, score: fuzzyScore(query, key(item)) }))
    .filter((x): x is { item: T; score: number } => x.score !== null)
    .sort((a, b) => b.score - a.score)
    .map((x) => x.item);
}
