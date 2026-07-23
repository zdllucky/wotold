#!/usr/bin/env bash
# PostToolUse hook для Write/Edit.
# По расширению редактируемого файла прогоняет быструю проверку:
#   .rs   → cargo fmt на файле + cargo check (incremental, timeout 60s)
#   .ts/.tsx → tsc --noEmit для соответствующего пакета (timeout 60s)
# Молча выходит для всего остального. Никогда не блокирует (exit 0 всегда),
# но печатает stdout/stderr — агент увидит ошибки и среагирует.
#
# Сценарии повышения шума отключены: cargo check --message-format short,
# tsc --pretty false, чтобы не загромождать output.

set -u

FILE="${CLAUDE_FILE_PATH:-${1:-}}"
[ -z "${FILE:-}" ] && exit 0
[ ! -f "$FILE" ] && exit 0

REPO="${CLAUDE_PROJECT_DIR:-/Users/Shared/projects/node/wotold}"

# Игнорируем правки вне репозитория проекта.
case "$FILE" in
  "$REPO"/*) ;;
  *) exit 0 ;;
esac

ext="${FILE##*.}"

# Подключаем cargo если его нет в PATH.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

case "$ext" in
  rs)
    if command -v cargo >/dev/null 2>&1; then
      MANIFEST="$REPO/apps/desktop/src-tauri/Cargo.toml"
      cargo fmt --manifest-path "$MANIFEST" -- "$FILE" >/dev/null 2>&1 || true
      timeout 60 cargo check \
        --manifest-path "$MANIFEST" \
        --message-format short \
        --quiet 2>&1 || true
    fi
    ;;
  ts|tsx)
    if command -v pnpm >/dev/null 2>&1; then
      case "$FILE" in
        "$REPO"/apps/desktop/*)
          timeout 60 pnpm --filter @wotold/desktop typecheck 2>&1 || true
          ;;
        "$REPO"/services/mcp/*)
          timeout 60 pnpm --filter @wotold/mcp typecheck 2>&1 || true
          ;;
        "$REPO"/packages/contracts/*)
          timeout 60 pnpm --filter @wotold/contracts exec tsc --noEmit 2>&1 || true
          ;;
      esac
    fi
    ;;
esac

exit 0
