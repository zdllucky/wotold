# Шрифты (self-hosted)

[TD-28] Раньше `styles/fonts.css` тянул семейства `@import`'ом с
`fonts.googleapis.com`: каждый запуск слал запрос в Google (fingerprint-утечка
у продукта, который продаётся приватностью), а без сети типографика молча
падала в системные шрифты. Теперь файлы лежат здесь и отдаются Vite по
`/fonts/`.

Всего 16 файлов, 148 КБ.

## Что скачано

| файл | семейство | вес | subset | размер |
|---|---|---|---|---|
| `onest-cyrillic-ext-400-800.woff2` | Onest | 400 800 | cyrillic-ext | 2.2 КБ |
| `onest-cyrillic-400-800.woff2` | Onest | 400 800 | cyrillic | 13.9 КБ |
| `onest-latin-ext-400-800.woff2` | Onest | 400 800 | latin-ext | 15.6 КБ |
| `onest-latin-400-800.woff2` | Onest | 400 800 | latin | 31.5 КБ |

Диапазон Onest расширен 400 700 → 400 800 для сайта (hero/h1 весом 800 по
handoff 2026-07-30). Файлы при этом **побайтно те же**: Google отдаёт полную
variable-ось независимо от запрошенного `wght`-диапазона, вес 800 в них был
всегда — его ограничивала только декларация `font-weight` в `fonts.css`.
Переименованы для честности имени.
| `ibm-plex-mono-cyrillic-ext-400.woff2` | IBM Plex Mono | 400 | cyrillic-ext | 4.2 КБ |
| `ibm-plex-mono-cyrillic-400.woff2` | IBM Plex Mono | 400 | cyrillic | 5.3 КБ |
| `ibm-plex-mono-latin-ext-400.woff2` | IBM Plex Mono | 400 | latin-ext | 8.7 КБ |
| `ibm-plex-mono-latin-400.woff2` | IBM Plex Mono | 400 | latin | 9.8 КБ |
| `ibm-plex-mono-cyrillic-ext-500.woff2` | IBM Plex Mono | 500 | cyrillic-ext | 4.2 КБ |
| `ibm-plex-mono-cyrillic-500.woff2` | IBM Plex Mono | 500 | cyrillic | 5.4 КБ |
| `ibm-plex-mono-latin-ext-500.woff2` | IBM Plex Mono | 500 | latin-ext | 8.6 КБ |
| `ibm-plex-mono-latin-500.woff2` | IBM Plex Mono | 500 | latin | 9.8 КБ |
| `ibm-plex-mono-cyrillic-ext-600.woff2` | IBM Plex Mono | 600 | cyrillic-ext | 4.2 КБ |
| `ibm-plex-mono-cyrillic-600.woff2` | IBM Plex Mono | 600 | cyrillic | 5.4 КБ |
| `ibm-plex-mono-latin-ext-600.woff2` | IBM Plex Mono | 600 | latin-ext | 8.8 КБ |
| `ibm-plex-mono-latin-600.woff2` | IBM Plex Mono | 600 | latin | 9.9 КБ |

## Почему Onest, а не Hanken Grotesk

[TD-47] У Hanken Grotesk, который был UI-шрифтом до 2026-07, **нет базовой
кириллицы** — Google отдаёт для него только `cyrillic-ext` (U+0460–052F,
историческая и расширенная). Диапазона U+0400–045F, то есть обычных русских
и казахских букв, в шрифте нет. Значит весь русский и казахский интерфейс —
основной язык продукта — рисовался системным `-apple-system`, а латиница
оставалась настоящим Hanken: две гарнитуры в одной строке.

Из шорт-листа (Inter / Onest / Manrope / Golos Text — все покрывают базовую
кириллицу и казахские Ұ/Ү) владелец выбрал **Onest** по внешнему виду.
Кириллица-first геометрическая гротеска, по темпераменту ближе всего к
прежнему Hanken.

Покрытие: `cyrillic`, `cyrillic-ext`, `latin`, `latin-ext`. Греческого и
вьетнамского в Onest нет — на момент решения продукту они не нужны (локали
ru/kk/en), расширять покрытие будем по мере появления самих языков.

IBM Plex Mono оставлен как есть: кириллицу покрывает полностью, включая
казахские Ұ/Ү (U+04B0–04B1).

## Как обновлять

Не руками. Subset'ы и `unicode-range` в `fonts.css` обязаны совпадать с тем,
что отдаёт Google, иначе часть символов молча перестанет попадать в файл:

```bash
curl -sSL -A "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36" \
  "https://fonts.googleapis.com/css2?family=Onest:wght@400..800&family=IBM+Plex+Mono:wght@400;500;600&display=swap"
```

Из ответа взять блоки нужных subset'ов, скачать `url(...)` и перегенерировать
`fonts.css` с теми же `unicode-range`.
