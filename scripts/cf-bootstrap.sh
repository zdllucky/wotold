#!/usr/bin/env bash
# Cloudflare bootstrap для wotold-proxy ([B8.3]).
#
# Создаёт KV-неймспейсы (QUOTA, AUTH) и R2-бакет для указанного environment.
# Печатает IDs которые нужно подставить в services/proxy/wrangler.toml (TODO_* плейсхолдеры).
#
# Usage:
#   ./scripts/cf-bootstrap.sh staging
#   ./scripts/cf-bootstrap.sh production
#
# Prereqs:
# - Cloudflare аккаунт (free tier OK)
# - wrangler CLI установлен глобально или доступен через pnpm: `pnpm dlx wrangler@4`
# - `wrangler login` — authenticated
# - Запускать из корня репозитория

set -euo pipefail

ENV="${1:-}"
if [[ -z "$ENV" ]]; then
  echo "usage: $0 <staging|production>"
  exit 1
fi
if [[ "$ENV" != "staging" && "$ENV" != "production" ]]; then
  echo "error: env must be 'staging' or 'production' (got: $ENV)"
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROXY_DIR="$REPO_ROOT/services/proxy"

if [[ ! -f "$PROXY_DIR/wrangler.toml" ]]; then
  echo "error: $PROXY_DIR/wrangler.toml not found"
  exit 1
fi

# Bucket name по env (matches wrangler.toml).
if [[ "$ENV" == "staging" ]]; then
  R2_BUCKET="wotold-stt-staging-stg"
else
  R2_BUCKET="wotold-stt-staging"
fi

cd "$PROXY_DIR"

echo "==> Creating KV namespace QUOTA (env=$ENV)"
echo "    Подставь id в wrangler.toml → [env.$ENV] → [[kv_namespaces]] binding=QUOTA"
pnpm dlx wrangler@4 kv namespace create QUOTA --env "$ENV" || true

echo
echo "==> Creating KV namespace AUTH (env=$ENV)"
echo "    Подставь id в wrangler.toml → [env.$ENV] → [[kv_namespaces]] binding=AUTH"
pnpm dlx wrangler@4 kv namespace create AUTH --env "$ENV" || true

echo
echo "==> Creating R2 bucket $R2_BUCKET (env=$ENV)"
pnpm dlx wrangler@4 r2 bucket create "$R2_BUCKET" || true

echo
echo "Done. Дальше — `wrangler secret put` для каждого секрета (см. docs/DEPLOYMENT.md)."
