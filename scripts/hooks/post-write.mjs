#!/usr/bin/env node
// PostToolUse hook для Write/Edit/MultiEdit.
// По расширению отредактированного файла прогоняет быструю проверку:
//   .rs      → cargo fmt на файле + cargo check (--message-format short)
//   .ts/.tsx → tsc --noEmit соответствующего workspace-пакета
// Молча выходит для всего остального. Никогда не блокирует (exit 0 всегда),
// но печатает вывод — агент увидит ошибку и среагирует.
//
// [TD-03] Переписан с bash на Node. Предыдущая версия (post-write.sh) была
// мёртвой с мая по двум независимым причинам:
//   1. читала переменную CLAUDE_FILE_PATH, которую Claude Code не выставляет —
//      payload приходит JSON'ом на stdin, как и у трёх соседних хуков;
//   2. оборачивала команды в `timeout 60`, а на macOS нет ни `timeout`, ни
//      `gtimeout` (GNU coreutils не в базовой системе) — то есть даже с
//      правильным путём к файлу каждый запуск падал бы в `command not found`,
//      проглоченный через `|| true`.
// Node решает обе: JSON.parse для stdin и spawnSync({timeout}) вместо coreutils.

import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const TIMEOUT_MS = 60_000;

/** Корень репозитория. CLAUDE_PROJECT_DIR — основной источник; фолбэк
 *  вычисляется от расположения самого хука (<repo>/scripts/hooks/post-write.mjs),
 *  а НЕ хардкодится — иначе в git-worktree хук молча пропускает все файлы. */
function repoRoot() {
  if (process.env.CLAUDE_PROJECT_DIR) return resolve(process.env.CLAUDE_PROJECT_DIR);
  return resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
}

function readStdin() {
  return new Promise((r) => {
    let buf = '';
    process.stdin.setEncoding('utf8');
    process.stdin.on('data', (c) => (buf += c));
    process.stdin.on('end', () => r(buf));
  });
}

/** Запускает команду с таймаутом, печатает вывод только если он есть. */
function run(cmd, args, cwd, label) {
  const res = spawnSync(cmd, args, {
    cwd,
    timeout: TIMEOUT_MS,
    encoding: 'utf8',
    env: { ...process.env, PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH ?? ''}` },
  });

  if (res.error?.code === 'ENOENT') return; // инструмента нет — не наша забота
  if (res.signal === 'SIGTERM') {
    process.stderr.write(`[post-write] ${label}: таймаут ${TIMEOUT_MS / 1000}s, пропускаем\n`);
    return;
  }
  const out = `${res.stdout ?? ''}${res.stderr ?? ''}`.trim();
  if (res.status !== 0 && out) process.stderr.write(`[post-write] ${label}:\n${out}\n`);
}

const raw = await readStdin();
let payload;
try {
  payload = JSON.parse(raw || '{}');
} catch {
  process.exit(0);
}

const file = payload?.tool_input?.file_path;
if (!file || !existsSync(file)) process.exit(0);

const REPO = repoRoot();
if (!resolve(file).startsWith(REPO + sep)) process.exit(0); // правки вне репо игнорируем

const ext = extname(file);
const rel = resolve(file).slice(REPO.length + 1);

if (ext === '.rs') {
  const manifest = `${REPO}/apps/desktop/src-tauri/Cargo.toml`;
  if (!existsSync(manifest)) process.exit(0);
  run('cargo', ['fmt', '--manifest-path', manifest, '--', file], REPO, 'cargo fmt');
  run(
    'cargo',
    ['check', '--manifest-path', manifest, '--all-targets', '--message-format', 'short', '--quiet'],
    REPO,
    'cargo check',
  );
} else if (rel.startsWith('apps/site/') && ['.ts', '.tsx', '.astro'].includes(ext)) {
  // У сайта свой чекер: astro check понимает .astro, а tsc — нет.
  if (!existsSync(`${REPO}/node_modules`)) {
    process.stderr.write('[post-write] node_modules нет — пропускаю astro check (pnpm install)\n');
    process.exit(0);
  }
  run('pnpm', ['--filter', '@wotold/site', 'check'], REPO, 'astro check');
} else if (ext === '.ts' || ext === '.tsx') {
  if (!existsSync(`${REPO}/node_modules`)) {
    process.stderr.write('[post-write] node_modules нет — пропускаю typecheck (pnpm install)\n');
    process.exit(0);
  }
  // packages/contracts потребляется как сырой TS (main: ./src/index.ts) и своего
  // tsc не имеет — проверяем его через потребителя, там же и ломается.
  const pkg = rel.startsWith('services/mcp/')
    ? '@wotold/mcp'
    : rel.startsWith('apps/desktop/') || rel.startsWith('packages/contracts/')
      ? '@wotold/desktop'
      : null;
  if (pkg) run('pnpm', ['--filter', pkg, 'typecheck'], REPO, `tsc ${pkg}`);
}

process.exit(0);
