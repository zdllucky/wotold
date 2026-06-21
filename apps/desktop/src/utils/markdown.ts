// Markdown helpers.

/**
 * Семантически-пустой markdown: только heading-строки (`# …`) и/или пробелы,
 * без реального тела. Нужно потому что v2 recap.md всегда начинается с
 * «# Рекап» — старый до-фиксный (пустой) рекап = `"# Рекап\n\n"`, строка
 * непустая, но контента нет. Без этой проверки MdPanel рендерил голый
 * заголовок и не показывал CTA «Пересоздать саммари».
 */
export function isMarkdownBlank(md: string | null | undefined): boolean {
  if (!md) return true;
  const body = md
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !/^#{1,6}\s/.test(line));
  return body.length === 0;
}
