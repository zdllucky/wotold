/**
 * Rehype-плагин правового шаблона (SPEC §7 handoff 2026-07-30).
 *
 * Контент legal/*.md написан юридическим форматом «1.1. Текст пункта» и
 * НЕ редактируется (правило «тексты не переписывать»). Номера в отдельную
 * mono-колонку выносит билд-слой:
 *
 *   - `<h2>1. Предмет</h2>` → `<h2><span class="no">1</span> Предмет</h2>`
 *   - `<p>1.1. Текст…</p>`  → `<div class="cl"><span class="n">1.1</span><p>Текст…</p></div>`
 *
 * Нумерация вида `6.1-бис` (terms.md) поддерживается. Всё остальное —
 * обычные абзацы, списки, callout'ы — не трогается. Плагин работает только
 * на файлах из content/docs/legal/.
 *
 * Без зависимостей: обычная рекурсия по hast-дереву вместо unist-util-visit.
 */

const H2_NO = /^(\d+)\.\s+/;
const CLAUSE_NO = /^(\d+\.\d+(?:-бис)?)\.\s+/;

const isElement = (node, tag) => node?.type === 'element' && node.tagName === tag;

/** Первый текстовый узел элемента (номер всегда стоит в начале). */
const firstText = (node) => {
  const child = node.children?.[0];
  return child?.type === 'text' ? child : null;
};

const span = (className, value) => ({
  type: 'element',
  tagName: 'span',
  properties: { className: [className] },
  children: [{ type: 'text', value }],
});

const transform = (node) => {
  if (!Array.isArray(node.children)) return;

  node.children = node.children.map((child) => {
    if (isElement(child, 'h2')) {
      const text = firstText(child);
      const match = text && text.value.match(H2_NO);
      if (match) {
        const rest = { ...text, value: text.value.slice(match[0].length) };
        return {
          ...child,
          children: [span('no', match[1]), rest, ...child.children.slice(1)],
        };
      }
      return child;
    }

    if (isElement(child, 'p')) {
      const text = firstText(child);
      const match = text && text.value.match(CLAUSE_NO);
      if (match) {
        const rest = { ...text, value: text.value.slice(match[0].length) };
        const p = { ...child, children: [rest, ...child.children.slice(1)] };
        return {
          type: 'element',
          tagName: 'div',
          properties: { className: ['cl'] },
          children: [span('n', match[1]), p],
        };
      }
      return child;
    }

    transform(child);
    return child;
  });
};

export default function rehypeLegal() {
  return (tree, file) => {
    const path = String(file?.path ?? '');
    if (!path.includes('/content/docs/legal/')) return;
    transform(tree);
  };
}
