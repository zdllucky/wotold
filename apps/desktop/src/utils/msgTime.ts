// [B26.8] Формат времени сообщения чата ассистента.
// Короткий: сегодня → «HH:MM», вчера → «<метка> HH:MM», раньше → «ДД.ММ.ГГГГ».
// Полный (по клику): «ДД.ММ.ГГГГ, HH:MM».
// Заметка: тик через полночь без таймера не обновится — перерендер по
// любому новому сообщению это чинит; принято сознательно.

function dayStart(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

function timeOf(d: Date, locale: string): string {
  return d.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' });
}

function dateOf(d: Date, locale: string): string {
  return d.toLocaleDateString(locale, { day: '2-digit', month: '2-digit', year: 'numeric' });
}

/** Короткий формат для облачка (низ-справа). */
export function formatMsgTime(
  iso: string,
  now: Date,
  locale: string,
  yesterdayLabel: string,
): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  const today0 = dayStart(now);
  const that0 = dayStart(d);
  const dayDiff = Math.round((today0 - that0) / 86_400_000);
  if (dayDiff <= 0) return timeOf(d, locale);
  if (dayDiff === 1) return `${yesterdayLabel} ${timeOf(d, locale)}`;
  return dateOf(d, locale);
}

/** Полный формат (раскрытие по клику): дата + время. */
export function formatMsgTimeFull(iso: string, locale: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  return `${dateOf(d, locale)}, ${timeOf(d, locale)}`;
}
