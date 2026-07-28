#!/usr/bin/env bash
# Dev-запуск с авто-перезапуском на изменения Rust-кода.
#
# `pnpm tauri dev` сам перезапускает приложение при правках `src-tauri/`, но
# на этом проекте цикл длинный: пересборка тянет sherpa-onnx, и на каждое
# сохранение уходит больше минуты. Этот скрипт даёт то же самое, но с двумя
# отличиями, которые экономят время:
#
#   1. Перед стартом гасит зависшие процессы прошлого запуска. Забытый vite
#      держит порт 5173, и тогда `beforeDevCommand` падает, а приложение молча
#      не поднимается — по логу это выглядит как «собралось и исчезло».
#   2. Watch только по `src-tauri/src`: правки фронта подхватывает vite сам,
#      пересобирать из-за них Rust незачем.
#
# Watcher опционален: без него скрипт работает как обычный `tauri dev`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="$ROOT/apps/desktop"
WATCH_DIR="$ROOT/apps/desktop/src-tauri/src"

# nvm-хук на .nvmrc ломается, если NVM_DIR пуст: он уходит ставить node
# заново в read-only каталог. Задаём явно.
export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"

cleanup_stale() {
  pkill -f "target/debug/wotold-desktop" 2>/dev/null || true
  pkill -f "tauri dev" 2>/dev/null || true
  pkill -f "vite" 2>/dev/null || true
  # Порт мог остаться занятым процессом, который не подошёл под маски выше.
  if command -v lsof >/dev/null 2>&1; then
    local pids
    pids="$(lsof -ti:5173 2>/dev/null || true)"
    [ -n "$pids" ] && kill $pids 2>/dev/null || true
  fi
  sleep 1
}

run_app() {
  cleanup_stale
  echo "▶ pnpm tauri dev"
  pnpm --dir "$APP" tauri dev
}

if [ "${1:-}" = "--once" ]; then
  run_app
  exit $?
fi

if command -v watchexec >/dev/null 2>&1; then
  echo "▶ watchexec: слежу за $WATCH_DIR (только .rs)"
  exec watchexec --restart --exts rs --watch "$WATCH_DIR" -- \
    bash -c "pkill -f 'target/debug/wotold-desktop' 2>/dev/null; pnpm --dir '$APP' tauri dev"
fi

if command -v entr >/dev/null 2>&1; then
  echo "▶ entr: слежу за $WATCH_DIR (только .rs)"
  while true; do
    find "$WATCH_DIR" -name '*.rs' | entr -d -r bash -c "pnpm --dir '$APP' tauri dev" || true
  done
fi

echo "ℹ watchexec/entr не найдены — запускаю без авто-перезапуска."
echo "  Поставить: brew install watchexec"
run_app
