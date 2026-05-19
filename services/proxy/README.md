# @wotold/proxy

Hono-сервис на Cloudflare Workers. Relay STT/LLM с подстановкой ключей владельца, квота по device-id, Free-тир. См. M9 и раздел 16.2 паспорта.

## Маршруты

| Маршрут | Что делает |
|---|---|
| `POST /v1/stt/staging-url` | Presigned PUT URL для аудио в R2 (R8 — аудио не через память воркера) |
| `POST /v1/stt` | Запуск транскрипции у партнёра (r2Key + opts) |
| `POST /v1/llm` | Relay в Anthropic, ожидается JSON-ответ |
| `GET /v1/usage` | Счётчики (SCAFFOLD, M9.4) |
| `GET /health` | Здоровье воркера |

Все маршруты `/v1/*` требуют заголовок `x-device-id` (UUID).

## Локальный запуск

```bash
pnpm install
cp .dev.vars.example .dev.vars   # заполнить ключи
pnpm dev
```

## Деплой

```bash
pnpm deploy   # wrangler deploy
```

Перед первым деплоем создать ресурсы и заполнить id в `wrangler.toml`:

```bash
wrangler kv namespace create QUOTA
wrangler r2 bucket create wotold-stt-staging
```

## Секреты (S1)

Через `wrangler secret put`:

- `ANTHROPIC_API_KEY` — ключ владельца для LLM
- `SONIOX_API_KEY` — primary STT
- `GLADIA_API_KEY` — fallback STT
- `R2_ACCOUNT_ID` — id Cloudflare-аккаунта для S3-endpoint R2
- `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY` — токен с RW в `wotold-stt-staging`

Ключи доступны **только** этой джобе деплоя, не сборке приложения.

## Принятые ограничения

- **R7**: на Free-тире при превышении лимитов воркер перестаёт отвечать без списаний. Норма.
- **R8**: аудио не проходит через память воркера. Только R2 + presigned URL.
- **M9.6**: контент не логируется. Только статусы и метрики по device-id.
- **M9.7**: BYO-путь не обслуживается — приложение ходит к партнёру напрямую.
- Cron Triggers / Durable Objects / длительные синхронные операции не использовать (Free-лимит).

## TODO

- Реальная привязка партнёрского STT (Soniox primary, Gladia fallback) в `src/routes/stt.ts`. Сейчас возвращает `501 provider_error`.
- Вёрстка тестов через `vitest` + miniflare (под Этап 8 follow-up).
- `// TODO(paid)` маркеры на ветках для платных тиров (R5).
