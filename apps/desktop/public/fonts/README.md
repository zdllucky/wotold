# Шрифты (self-hosted)

[TD-28] Раньше `styles/fonts.css` тянул семейства `@import`'ом с
`fonts.googleapis.com`: каждый запуск слал запрос в Google (fingerprint-утечка
у продукта, который продаётся приватностью), а без сети типографика молча
падала в системные шрифты. Теперь файлы лежат здесь и отдаются Vite по
`/fonts/`.

Всего 18 файлов, 157 КБ.

## Что скачано

| файл | семейство | вес | subset | размер |
|---|---|---|---|---|
| `manrope-cyrillic-ext-400-700.woff2` | Manrope | 400 700 | cyrillic-ext | 2.5 КБ |
| `manrope-cyrillic-400-700.woff2` | Manrope | 400 700 | cyrillic | 14.2 КБ |
| `manrope-greek-400-700.woff2` | Manrope | 400 700 | greek | 9.1 КБ |
| `manrope-vietnamese-400-700.woff2` | Manrope | 400 700 | vietnamese | 8.3 КБ |
| `manrope-latin-ext-400-700.woff2` | Manrope | 400 700 | latin-ext | 14.9 КБ |
| `manrope-latin-400-700.woff2` | Manrope | 400 700 | latin | 24.0 КБ |
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

## Почему Manrope, а не Hanken Grotesk

[TD-47] У Hanken Grotesk, который был UI-шрифтом до 2026-07, **нет базовой
кириллицы** — Google отдаёт для него только `cyrillic-ext` (U+0460–052F,
историческая и расширенная). Диапазона U+0400–045F, то есть обычных русских
и казахских букв, в шрифте нет. Значит весь русский и казахский интерфейс —
основной язык продукта — рисовался системным `-apple-system`, а латиница
оставалась настоящим Hanken: две гарнитуры в одной строке.

Из шорт-листа (Inter / Onest / Manrope / Golos Text — все покрывают базовую
кириллицу и казахские Ұ/Ү) выбран **Manrope** по ширине покрытия: cyrillic,
cyrillic-ext, greek, latin, latin-ext, vietnamese. Побочно набор стал легче —
73 КБ против 139 КБ, потому что вариативный файл один на subset.

IBM Plex Mono оставлен как есть: кириллицу он покрывает полностью, включая
казахские Ұ/Ү (U+04B0–04B1).

## Как обновлять

Не руками. Subset'ы и `unicode-range` в `fonts.css` обязаны совпадать с тем,
что отдаёт Google, иначе часть символов молча перестанет попадать в файл:

```bash
curl -sSL -A "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36" \
  "https://fonts.googleapis.com/css2?family=Manrope:wght@400..700&family=IBM+Plex+Mono:wght@400;500;600&display=swap"
```

Из ответа взять блоки нужных subset'ов, скачать `url(...)` и перегенерировать
`fonts.css` с теми же `unicode-range`.
