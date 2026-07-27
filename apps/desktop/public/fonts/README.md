# Шрифты (self-hosted)

[TD-28] Раньше `styles/fonts.css` тянул семейства `@import`'ом с
`fonts.googleapis.com`. Для local-first приложения это значило две вещи:
каждый запуск слал запрос в Google (fingerprint-утечка у продукта, который
продаётся приватностью), а без сети типографика молча падала в системные
шрифты. Теперь файлы лежат здесь и отдаются Vite по `/fonts/`.

Всего 15 файлов, 139 КБ.

## Что скачано

| файл | семейство | вес | subset | размер |
|---|---|---|---|---|
| `hanken-grotesk-cyrillic-ext-400-700.woff2` | Hanken Grotesk | 400 700 | cyrillic-ext | 1.6 КБ |
| `hanken-grotesk-latin-ext-400-700.woff2` | Hanken Grotesk | 400 700 | latin-ext | 19.1 КБ |
| `hanken-grotesk-latin-400-700.woff2` | Hanken Grotesk | 400 700 | latin | 33.9 КБ |
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

Вьетнамский subset **не** скачан намеренно — локали ru/kk/en его не
требуют, это сэкономило ~40 КБ.

## Известное ограничение: Hanken Grotesk без базовой кириллицы

У Hanken Grotesk есть только `cyrillic-ext` (U+0460–052F) — это историческая
и расширенная кириллица. **Базового диапазона U+0400–045F, то есть обычных
русских и казахских букв, в шрифте нет.**

Значит весь русский и казахский интерфейс рендерится системным фолбэком
(`-apple-system`), а не Hanken Grotesk — и так было всегда, self-hosting
этого не изменил, только сделал явным. Латиница и цифры — настоящий Hanken.

IBM Plex Mono кириллицу имеет полностью, включая казахские Ұ/Ү
(U+04B0–04B1), так что таймкоды и идентификаторы набраны как задумано.

Решение (менять UI-шрифт на кириллический или принять фолбэк) —
продуктовое, в TECH_DEBT заведено отдельной задачей.

## Как обновлять

Не руками. Subset'ы и `unicode-range` в `fonts.css` обязаны совпадать с тем,
что отдаёт Google для тех же семейств, иначе часть символов молча перестанет
попадать в нужный файл:

```bash
curl -sSL -A "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36" \
  "https://fonts.googleapis.com/css2?family=Hanken+Grotesk:wght@400..700&family=IBM+Plex+Mono:wght@400;500;600&display=swap"
```

Из ответа взять блоки нужных subset'ов, скачать `url(...)` и перегенерировать
`fonts.css` с теми же `unicode-range`.
