# M12 Local Engine — UX/CX Design Brief

> **Тип:** Запрос на проработку UX/CX от backend → дизайнеру.
> **Версия:** 0.1 (2026-05-22)
> **Источник проблем:** Live-тест собранного M12 в `tauri dev`. Реальные whisper.cpp + llama.cpp + pyannote модели скачаны, pipeline проходит, но клиентский flow ощущается шероховатым.
>
> **Что от тебя нужно:** Design-артефакты (wireframes / Figma / handoff-spec) на изменения **под существующую систему Atelier v2** (`docs/design/atelier-v2/`). Все 6 theme×accent комбинаций (light/dark × bordeaux/persian/ink) должны работать. Не предлагай новых токенов без необходимости.
>
> **Что не делать:** не переписывай весь UX с нуля. M12 — аддендум к уже задеплоенной cloud-логике; нам надо вписать local-движок так, чтобы он не сломал ритуалы и язык продукта (Atelier v2 editorial direction, Source Serif 4 + DM Sans + JetBrains Mono).

---

## 0. Контекст в одной фразе

Wotold — десктоп-приложение записи звонков. M12 добавляет **третий движок обработки** — полностью локальный (sherpa-onnx Whisper + llama.cpp), который скачивает модели по требованию и работает без сети. Cloud (Soniox + Anthropic через прокси) сохранён как Pro-опция.

Полное ТЗ движка: [`docs/M12_LOCAL_ENGINE_PRD.md`](../M12_LOCAL_ENGINE_PRD.md) v0.2. Декомпозиция готового кода: [`docs/ROADMAP.md`](../ROADMAP.md) §M12.

**Что уже реализовано (UI surfaces для ревизии):**

| Файл | Что это |
|---|---|
| [`apps/desktop/src/pages/OnboardingPage.tsx`](../../apps/desktop/src/pages/OnboardingPage.tsx) | 4-step onboarding для macOS (был 3-step) |
| [`apps/desktop/src/pages/OnboardingEngineStep.tsx`](../../apps/desktop/src/pages/OnboardingEngineStep.tsx) | Новый step «Движок» между Owner и Permissions |
| [`apps/desktop/src/pages/LocalEngineSection.tsx`](../../apps/desktop/src/pages/LocalEngineSection.tsx) | Settings → «Движок распознавания» (engine picker + preset + storage) |
| [`apps/desktop/src/pages/HomePage.tsx`](../../apps/desktop/src/pages/HomePage.tsx) | Existing-users announcement banner |
| [`apps/desktop/src/i18n/{ru,en,kk}.ts`](../../apps/desktop/src/i18n/ru.ts) | Все строки M12 в `localEngine.*`, `onboarding.engine.*`, `home.engineAnnouncement*` |

---

## 1. Проблемы клиентского пути (по flow)

### 1.1 Onboarding · Engine setup step (4-й шаг на macOS)

**Текущее состояние.** После Owner info user видит:

- Eyebrow `Шаг 03 из 04 · Движок`
- `.display`: «Подбираем движок под ваш Mac»
- `.subtitle`: про local vs cloud
- `.index-card`: «Ваш Mac · M2 Pro · 16 GB · с Metal» + рекомендация preset + 3 буллита что входит
- Кнопки вертикально:
  - 🟥 Primary «Скачать и продолжить (~2.4 GB)»
  - ⬜ Ghost «Выбрать другой пресет»
  - 🔘 Quiet «Использовать облако вместо локального»

**Проблемы:**

1. **3 CTA одного веса.** Primary визуально близок к Ghost/Quiet, выбор «по умолчанию» неочевиден. Я хочу чтобы рекомендуемый путь (Download) был **очевидно главным**, остальные — escape hatches, не равноценные альтернативы.
2. **Сюрприз размера.** «~2.4 GB» в кнопке — это много для первого впечатления. User'у не показано: «Скачается 1 раз. После — без интернета». Нет comparison «зато cloud считает каждую минуту, плюс на твой бизнес плана нужно $X».
3. **Probe-результат сухой.** `M2 Pro · 16 GB · с Metal` — это диагностический язык. User не понимает «хорошо ли это для меня». Нужна **эмоциональная reassurance**: «Отличный Mac для локального движка ✓».
4. **«Выбрать другой пресет» раскрывается inline.** При экспансии форма вырастает, кнопки уезжают вниз, контекст теряется. Это нарушение Atelier v2 ритма (single-surface focus).
5. **Cancel during download** — мы показываем `.activity-strip` с «качаем X MB/s» + cancel. Но user не понимает: «если отменю — потеряю прогресс? могу продолжить позже?». Сейчас cancel → engine=cloud_managed + cleanup partial. Это **destructive по факту**, но UI говорит «продолжить с облаком», что звучит мягко.
6. **Нет sample / preview.** Перед скачиванием 2.4 GB неплохо было бы показать «вот пример распознавания» — 10-секундный демо-аудио + получившийся transcript. Сейчас user покупает кота в мешке.

**Желаемый исход** (не решение — направление):
- Один очевидный путь вперёд + один escape «не сейчас, использую облако».
- Probe-результат на человеческом языке + reassurance.
- Размер download'а в контексте: «один раз, потом без сети».
- Preview возможностей (опционально).
- Cancel honest: предупреждение что engine переключится на cloud.

---

### 1.2 Settings → Движок распознавания

**Текущее состояние** (см. [LocalEngineSection.tsx](../../apps/desktop/src/pages/LocalEngineSection.tsx)):

1. Hardware probe баннер (одной строкой `.subtle`): «Apple M2 Pro · 16 GB · Metal — рекомендуем Balanced» + «Переоценить».
2. **Engine picker** — 3 `.index-card` радио-карточки:
   - **Local** · бесплатно, приватность, ваше железо · ●●○ качество
   - **Cloud** · Pro · топ-качество, без локальной нагрузки · ●●● качество
   - **Свои ключи** · напрямую к партнёрам · ●●● качество
3. При выборе Local — раскрывается **preset picker** (Light/Balanced/Quality) с `.dot--{success,accent,muted}` статусом моделей.
4. Снизу — «Установлено: 2.4 GB» + кнопка «Освободить место» → modal со списком моделей.
5. Optional Hw banner (`.activity-strip`) поверх с «Применить» если probe рекомендует другой preset.

**Проблемы:**

1. **3 уровня иерархии путаются.** Engine picker → Preset picker → Storage table. Каждый уровень — своя ментальная модель. User часто хочет «просто запустить» — а тут три выбора.
2. **`●●○` / `●●●` индикаторы качества.** Что это? 3 кружка из скольки? «Качества чего» — STT? LLM? Общее ощущение? Без референса непонятно. Дизайнер из Apple Hardware UI сделал бы это star-rating или explicit % / SLA labels.
3. **«Quality» preset trap.** Слово «Quality» imply «лучшее». User-natural выбор — Quality. Но он 7.5 GB и медленный — Balanced рекомендован для 90% юзеров. Probe пытается это сказать через банер, но иерархия слов работает против нас.
4. **Storage management — modal.** Это «секонд-класс UX» — пользователь не видит inline что у него установлено. iOS Settings → iCloud Storage показывает таблицу прямо на странице. Тут она прячется за кнопкой.
5. **Active model badge.** В storage modal активная модель помечена «активна», но это не текстовый ярлык а просто `.dot--success`. Слабо различимо. Удаление активной модели → confirm → но user не видит «что будет вместо» (next preset / cloud fallback).
6. **«Переоценить» кнопка `.btn--quiet`.** Если probe возвращает странные данные (например ram=0 на virtualized Mac → recommendation=null), как user поймёт что probe сломался? Сейчас просто disappear'ит banner.
7. **«Все остальные акценты cloud-related»** — например квота `/v1/usage` в M9.5 — должны прятаться когда выбран Local. Сейчас в Settings они остаются видимыми (M9.6 todo).
8. **Engine kind labels.** «Cloud · Pro» — но Pro tier ещё не запущен, billing = stub (R5). User может ожидать платный flow, увидеть пустоту, разочароваться. Нужны honest labels: «Cloud (free tier, ограничен X мин/день)» vs «Cloud Pro (готовится)».

**Желаемый исход:**
- Один primary state + secondary settings. Преsеt выбор — visible inline, не за раскрытием.
- Quality indicators self-explanatory (не abstract dots).
- Storage table inline (не в modal) когда выбран Local.
- Honest labels про статусы (Pro вышел / нет).
- Confirm «удалить активную модель» с явным «вместо неё используем …».

---

### 1.3 HomePage announcement banner (для existing users)

**Текущее состояние** ([HomePage.tsx](../../apps/desktop/src/pages/HomePage.tsx) §M12.7.5):

```
┌─────────────────────────────────────────────────┐
│ ПОЯВИЛСЯ ЛОКАЛЬНЫЙ РЕЖИМ                        │
│ Теперь Wotold может работать полностью          │
│ на устройстве, без облака — бесплатно навсегда. │
│ Попробовать?                                    │
│                                  [Открыть] [Позже] │
└─────────────────────────────────────────────────┘
```

**Проблемы:**

1. **Лоу-эффорт CTA.** «Открыть» ведёт в Settings → Engine. Но это generic Settings page — user должен сам найти Local card. Должно быть deep-link с фокусом + scroll-into-view + visual flash.
2. **Никакого визуала.** Просто текст. У нас Atelier v2 с editorial direction — баннер мог бы быть более sumptuous. Иконка / illustration / mini-graph «$/мес → 0$».
3. **Нет «before/after».** User'у не показано: «вот сейчас твоя квота — 75% использовано. После переключения — без лимитов».
4. **Dismiss permanent.** Closed once = never again. А что если user dismissed случайно? Нужна re-discoverability через что-то в Settings или Help menu.
5. **Когда показывается.** Сейчас: при ≥1 ready call. А если у user'а 0 successful calls (только failures)? Он наоборот должен увидеть local баннер первым делом.

**Желаемый исход:**
- Visually richer (но в рамках Atelier v2 editorial).
- Concrete benefit (например, текущий usage → 0$ tomorrow).
- Smart показ: при failure spike, при quota approach, при first record after update.
- Re-discoverable: если dismiss — где найти потом?

---

### 1.4 Pipeline running (когда Local engine processing)

**Текущее состояние:** В CallDetailPage / CallsPage processing-status row показывает `pipeline_step` (1-5) + pct + ETA.

Для Local route стейджи:
- 1 Upload (instant pseudo-step)
- 2 Transcribe (длинный — STT обоих треков)
- 3 RecognizeSpeakers (cluster через WeSpeaker)
- 4 MergeArtifacts (instant)
- 5 Recap (LLM, может минуту+)

**Проблемы:**

1. **Stage 1 «Upload» вводит в заблуждение в Local-режиме.** В cloud режиме это «загружаем в R2». В local — это «загружаем модель в RAM». User не понимает что именно происходит.
2. **Прогресс не учитывает «два провайдера параллельно».** Мы транскрибируем mic + system одновременно, но прогресс показывает один общий pct. Если mic закончился раньше — user не видит этого.
3. **Time estimate отсутствует.** Local на M2 Pro для 30-минутного звонка занимает ~5-7 минут (STT) + ~30 секунд (LLM). Этот ETA можно прикидывать **до старта** по длительности файла. Сейчас просто spinner.
4. **Failure copy.** На fail из `local_engine_stt_failed` / `local_llm_timeout` мы показываем raw `failed_reason` через `humanError`. Маркеры вроде `local_engine_model_missing` пользователю не нужны — нужны actions (скачать, переключить на cloud, retry).
5. **Что произошло на M12-D5 sortformer fail?** Если pyannote не скачан → degraded mode (system track = `speaker:0`). User видит ready call, но в Speakers tab только 1 «спикер» вместо 3-4. Никакого indicator что diarization работала в fallback.

**Желаемый исход:**
- Stage labels человечные («Слушаю запись», «Узнаю кто говорит», «Пишу саммари»).
- Параллельные шаги отображены параллельно.
- ETA до старта по duration + railway-track-like прогресс.
- Failure reasons → actions, не markers.
- Degraded mode (diarization off) → soft notice «обработано в упрощённом режиме — подумайте установить full preset».

---

### 1.5 Cross-cutting

1. **Микрокопи.** «Движок распознавания» звучит технически. Альтернативы из других продуктов: «Как обрабатывать», «Скорость и приватность», «Где работает запись». Нужен tonality pass.
2. **Pricing language.** Все упоминания «Free / Pro / BYO» сейчас inconsistent. R5 паспорта говорит что billing = stub в MVP, но UI местами показывает «Pro», что user может прочесть как «купить». Нужен honest язык: «Свободный тариф», «Сборка моделей», «Облако через прокси Wotold», «Свои ключи провайдеров».
3. **i18n consistency.** ru/kk/en все три есть, но некоторые ключи длиннее в kk (Kazakh — длиннее ru в 1.2-1.5x), что ломает узкие колонки. Нужен audit + maybe shorter labels в проблемных местах.
4. **Empty / loading states.** При первом открытии Settings → Engine, пока probe ещё идёт, пока catalog подгружается — что user видит? Сейчас probably blank или skeleton. Нужен deliberate flow.
5. **Accessibility.** `.dot--*` индикаторы (color-only) — для screen reader / dark color blindness не работают. Нужен текст или icon.
6. **Onboarding для non-macOS.** На Linux/Windows step «Движок» скипается (R9). Но user не получает объяснения почему. Может быть «Local engine скоро» soft-mention.

---

## 2. Технические рамки (что нельзя ломать)

1. **Atelier v2 design system.** Все токены — `var(--*)` из `tokens.css`. Component classes — `.btn`, `.card`, `.index-card`, `.dot`, `.field`, `.activity-strip`, `.modal-backdrop`, `.tabs`, etc. См. [`apps/desktop/src/styles/wotold.css`](../../apps/desktop/src/styles/wotold.css). Никаких новых токенов без явного предложения.
2. **6 theme × accent комбинаций.** Light/dark × bordeaux/persian/ink. Любой новый surface — 6 скринов.
3. **Source Serif 4 (display/title/transcript) + DM Sans (UI) + JetBrains Mono (timestamps/IDs).** No new fonts.
4. **`var(--signal)` (красный) — ТОЛЬКО recording state + destructive actions.** Не для CTAs / errors / hover.
5. **`var(--accent)` (bordeaux/persian/ink) — для всего UI.** Не миксовать.
6. **Reduced-motion.** Все анимации respect `prefers-reduced-motion`. См. existing patterns в `RecBtn`, `dot--pulse`, `Coachmarks`.
7. **Accessibility floor.** WCAG 2.2 AA. Все pickers должны иметь aria-labelledby. Modal — focus trap (используем `useFocusTrap` hook).
8. **Engine kind values стабильны.** `'local'` / `'cloud_managed'` / `'cloud_byo'` — string literals в settings, не менять.
9. **Preset stable.** `'light'` / `'balanced'` / `'quality'` — заперты в Rust enum + i18n. Можно переименовать UI-label, но keys остаются.
10. **HW probe result schema** — см. `HwReport` в [`packages/contracts/src/local-engine.ts`](../../packages/contracts/src/local-engine.ts). `recommendation: null` валидное значение (probe не смог определить).

---

## 3. Что нужно от тебя (дизайнера) — deliverables

### Базовый минимум (must-have)

1. **Onboarding · Engine setup step (overhaul).** Wireframe + final design (Figma либо handoff-md) для 4-step flow:
   - State A: probe вернул recommendation, models не скачаны (default).
   - State B: download running (progress + ETA + cancel honest).
   - State C: model already cached (resume / continue).
   - State D: probe failed / non-macOS (graceful skip).
2. **Settings → Движок распознавания (rework).** Inline storage, лучшая иерархия, quality indicators self-explanatory. Все 6 theme×accent.
3. **HomePage announcement banner v2.** Visually richer, deep-link с фокусом на Local card в Settings.
4. **Pipeline progress UI для Local route.** Stage labels на человеческом + параллельный STT + ETA от duration.
5. **Microcopy pass.** ru/kk/en — все строки `localEngine.*` + `onboarding.engine.*` + `home.engineAnnouncement*` (см. `apps/desktop/src/i18n/ru.ts`). Финальные тексты с консистентной тональностью.

### Желательно (nice-to-have)

6. **Failure recovery flows.** Что user видит на:
   - `local_engine_model_missing` → CTA «Скачать» / «Переключиться на cloud для этого звонка».
   - `local_whisper_timeout` / `local_llm_timeout` → retry с soft hint про preset.
   - Degraded diarization → soft notice.
7. **Preview / sample** перед download'ом в onboarding (10-сек аудио → transcript).
8. **Existing-user reactivation flow.** Where to surface local mode invite после первого dismiss.

### Что НЕ нужно (out of scope)

- Не переделывай Cloud/BYO flow — он шипнут и работает.
- Не предлагай переписывать voice biometrics (B3.x) — отдельная история.
- Не трогай recording UX (W3-W6 baked).
- Не предлагай новых fonts / accent colors — Atelier v2 — это контракт.

---

## 4. Формат финального артефакта

Когда готово — отдай мне один из вариантов:

1. **Figma file** + handoff Markdown в `docs/design/atelier-v2/m12-handoff.md` с разделами per surface (Onboarding · Engine, Settings · Engine, HomePage · Banner, Pipeline · Progress, Storage · Inline, Failure flows).
2. **ASCII / Markdown wireframes** в том же `m12-handoff.md` если Figma overkill.

В любом случае handoff должен содержать **для каждого surface:**

- Описание задачи (1-2 предложения)
- Состояния (default / loading / success / failure / disabled / etc)
- Используемые tokens + classes (из existing system)
- Новые tokens (если действительно нужны — обоснование)
- i18n keys + финальные ru/en/kk строки
- Accessibility notes (labels, focus order, screen reader announcements)
- 6 theme×accent verify checklist

Я приму handoff, прогоню design-gate (см. [`.claude/skills/design-gate/SKILL.md`](../../.claude/skills/design-gate/SKILL.md)) и реализую под существующий код.

---

## 5. Контекст принятых ограничений (раздел 12 паспорта + R-серия M12)

Кратко то, что мы **сознательно не чиним** в MVP — дизайн должен учитывать:

- **R9** Local-движок только macOS. На Linux/Windows — engine step скипается.
- **R10** Модели не бандлятся в installer. Download on-demand.
- **R11** Real-time / streaming local STT — нет (offline только).
- **R12** Качество local-LLM ниже cloud. UI **обязан** это явно показывать.
- **R12-bis** Авто-удаление моделей при switch preset — никогда. Только manual.
- **R13** Слабое железо НЕ блокирует Local — только warning.

См. [`docs/M12_LOCAL_ENGINE_PRD.md`](../M12_LOCAL_ENGINE_PRD.md) §5 для полного описания.

---

## 6. Текущее состояние работы (для контекста, не для копи-паста)

- Backend (Rust + Tauri) — все 28 M12 задач закрыты, pipeline end-to-end рабочий с real whisper.cpp + llama.cpp + pyannote.
- Frontend — все surfaces реализованы и проходят TS typecheck + vitest. Дизайн **рабочий, но грубый** — что и привело к этому brief'у.
- Реальный тест на M2 Pro 16 GB прошёл: download → onboarding → record → pipeline → ready. UX feedback: «не удобно».

Поэтому — дизайнерская итерация перед очередным дев-циклом.

---

**Конец brief'а v0.1.**

Если требуются скриншоты текущих surfaces — могу прислать. Если нужны логи / DB state — тоже могу. Если хочешь — могу сделать short screencast как user проходит onboarding.
