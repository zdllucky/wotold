// [B27] Пользовательский запрос → выражение FTS5 MATCH.
//
// SECURITY: сырой ввод в MATCH — это не «поиск строки», а исполнение
// синтаксиса FTS5: кавычки, `*`, `:`, `NEAR`, `AND/OR/NOT` меняют смысл
// запроса, а незакрытая кавычка роняет запрос ошибкой парсера. Поэтому
// каждый токен уходит в MATCH закавыченным.
//
// Разбор повторяет `assistant/retrieval.rs::build_match_expr` (правило twin
// parity): те же пороги, та же префикс-экспансия — иначе один и тот же
// вопрос давал бы в MCP и в приложении разные результаты.

/** Односимвольные токены — шум (предлоги). */
const MIN_TOKEN_CHARS = 2;
/** Потолок токенов: запрос-простыня не должен разносить bm25. */
const MAX_TOKENS = 12;
/** Слова от этой длины получают префикс-экспансию (морфология-lite). */
const PREFIX_EXPANSION_MIN_CHARS = 6;
/** Минимальная длина основы при экспансии. */
const PREFIX_STEM_MIN_CHARS = 4;

function termForToken(token: string): string {
  const chars = Array.from(token);
  if (chars.length >= PREFIX_EXPANSION_MIN_CHARS) {
    const stemLen = Math.max(chars.length - 2, PREFIX_STEM_MIN_CHARS);
    return `"${chars.slice(0, stemLen).join('')}"*`;
  }
  return `"${token}"`;
}

/**
 * Запрос → MATCH-выражение, либо `null` если искать нечего (пусто, знаки
 * препинания, односимвольные слова). `null` означает «не ходить в БД»,
 * а не «ничего не найдено».
 */
export function buildMatchExpr(query: string): string | null {
  const tokens = query
    .split(/[^\p{L}\p{N}]+/u)
    .filter((w) => Array.from(w).length >= MIN_TOKEN_CHARS)
    .slice(0, MAX_TOKENS);
  if (tokens.length === 0) return null;
  return tokens.map(termForToken).join(' OR ');
}
