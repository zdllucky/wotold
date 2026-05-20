# Deployment — Wotold backend (Cloudflare Workers)

Двухэтапный пайплайн с двумя environments на бесплатном Cloudflare-плане (R7 паспорта).

| Environment | Триггер | Worker URL |
|---|---|---|
| **staging** | push в `main` (после `preflight` job) | `wotold-proxy-staging.workers.dev` |
| **production** | tag `v*.*.*` (синхронно с релизом desktop-приложения) | `wotold-proxy.workers.dev` |

Frontend = desktop-приложение, **не web** — деплой через GitHub Releases + Tauri updater (см. `release-app.yml` и М11 паспорта).

## 0. Single-time setup

### 0.1 Cloudflare account

1. Завести бесплатный аккаунт на [cloudflare.com](https://www.cloudflare.com/).
2. Создать **API Token** с разрешениями:
   - `Account → Workers Scripts → Edit`
   - `Account → Workers KV Storage → Edit`
   - `Account → Workers R2 Storage → Edit`
3. Сохранить **Account ID** (видно в dashboard → правое верхнее меню).

### 0.2 Локальный wrangler

```bash
pnpm dlx wrangler@4 login   # OAuth-флоу в браузере
```

### 0.3 Bootstrap инфра per-env

Скрипт `scripts/cf-bootstrap.sh` создаёт KV-неймспейсы и R2-бакет для указанного окружения.

```bash
./scripts/cf-bootstrap.sh staging
./scripts/cf-bootstrap.sh production
```

После каждого запуска **вручную подставить полученные `id`** в `services/proxy/wrangler.toml`, заменив плейсхолдеры:

```toml
[[env.staging.kv_namespaces]]
binding = "QUOTA"
id = "TODO_STAGING_KV_QUOTA_ID"   # ← подставить реальный id

[[env.staging.kv_namespaces]]
binding = "AUTH"
id = "TODO_STAGING_KV_AUTH_ID"    # ← подставить реальный id
```

Аналогично для `[[env.production.kv_namespaces]]`.

### 0.4 Секреты per-env через `wrangler secret put`

```bash
cd services/proxy

# STAGING
pnpm dlx wrangler@4 secret put ANTHROPIC_API_KEY            --env staging
pnpm dlx wrangler@4 secret put SONIOX_API_KEY               --env staging
pnpm dlx wrangler@4 secret put GLADIA_API_KEY               --env staging
pnpm dlx wrangler@4 secret put R2_ACCOUNT_ID                --env staging
pnpm dlx wrangler@4 secret put R2_ACCESS_KEY_ID             --env staging
pnpm dlx wrangler@4 secret put R2_SECRET_ACCESS_KEY         --env staging
pnpm dlx wrangler@4 secret put GOOGLE_OAUTH_CLIENT_SECRET   --env staging
# APPLE_OAUTH_CLIENT_SECRET / MICROSOFT_OAUTH_CLIENT_SECRET — пока не нужны (X4 deferred).

# PRODUCTION — то же самое с --env production. Для безопасности использовать
# отдельные ключи (separate Soniox API key, separate Anthropic key и т.д.).
```

### 0.5 GitHub Repository secrets + Environments

В GitHub UI: **Settings → Environments → New environment**, создать `staging` и `production`.

**Account-level secrets** (Repository secrets, доступны обоим environments):

| Secret | Назначение |
|---|---|
| `CLOUDFLARE_API_TOKEN` | API token из шага 0.1 |
| `CLOUDFLARE_ACCOUNT_ID` | Account ID из шага 0.1 |

**Environment-level protection rules** (опционально, для production):

- `production`: добавить required reviewer'ов (нужен manual approval перед каждым деплоем).
- `staging`: rules не нужны (auto-deploy on main push).

### 0.6 OAuth client IDs (M10 паспорта)

`GOOGLE_OAUTH_CLIENT_ID`, `APPLE_OAUTH_CLIENT_ID`, `MICROSOFT_OAUTH_CLIENT_ID` — публичные ID (не secrets), задаются в `wrangler.toml` через `[env.*.vars]`. Сейчас пусто — подставить когда будет нужно (X4 follow-up).

`*_OAUTH_CLIENT_SECRET` — секреты, ставятся через `wrangler secret put` (см. 0.4).

### 0.7 R2 lifecycle (опционально)

STT staging-объекты живут временно. На бесплатном плане Cron Triggers ограничены — настраиваем lifecycle через bucket rule (вне wrangler):

1. CF dashboard → R2 → выбрать бакет → Settings → Object lifecycle
2. Rule: delete objects after **3 days**

## 1. Daily flow

### 1.1 PR

1. `pnpm --filter @wotold/proxy test` — локально проверить (vitest unit + миниframe integration).
2. PR в `main` — CI запустит `preflight` job (typecheck + tests). Деплой не происходит.

### 1.2 Merge в main → staging

`deploy-proxy.yml` автоматически:

1. Запускает preflight.
2. Деплоит на `wotold-proxy-staging.workers.dev` через `wrangler deploy --env staging`.

Проверить health: `curl https://wotold-proxy-staging.workers.dev/health`

### 1.3 Production release

Production деплоится только по tag `v*.*.*`:

```bash
git tag v0.1.0
git push origin v0.1.0
```

`deploy-proxy.yml` запустит preflight + `deploy-production` job. С GitHub Environment protection — потребует approval перед деплоем.

В тот же момент `release-app.yml` соберёт и подпишет desktop-приложение для GitHub Releases (синхронная версия по M11.5).

### 1.4 Manual override

Запустить деплой вручную через GitHub UI:

**Actions → Deploy Proxy → Run workflow → environment: staging|production**

## 2. Free tier лимиты

| Ресурс | Лимит | Использование |
|---|---|---|
| Workers | 100k req/день | OIDC + STT relay + LLM relay |
| KV reads | 100k/день | quota check + session lookup |
| KV writes | 1k/день | quota inc + sessions + accounts |
| KV storage | 1GB | sessions + accounts |
| R2 storage | 10GB | STT staging (с lifecycle 3 дня) |
| R2 Class A ops | 10M/мес | PUT + LIST |
| R2 Class B ops | 1M/мес | GET + HEAD |
| Cron Triggers | 3 шт | пока не используем |

**Запреты Free**: Durable Objects, Email Workers. Не использовать.

При приближении к лимитам — CF dashboard покажет; ужать quota в `wrangler.toml`.

## 3. Rollback

```bash
cd services/proxy
pnpm dlx wrangler@4 rollback --env production
```

Откатит на предыдущий deployment. KV/R2 данные не трогаются.

## 4. Local dev

Запуск прокси локально с in-memory KV/R2 через миниframe:

```bash
cd services/proxy
cp .dev.vars.example .dev.vars   # one-time — заполнить secrets
pnpm dlx wrangler@4 dev          # http://localhost:8787
```

`.dev.vars` в `.gitignore` — никогда не коммитим.

Тесты:

```bash
pnpm --filter @wotold/proxy test               # unit + integration
pnpm --filter @wotold/proxy test:integration   # only миниframe routes
```
