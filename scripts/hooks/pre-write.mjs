#!/usr/bin/env node
// PreToolUse hook для Write/Edit.
// Блокирует:
//   - попытки правки файлов с секретами (Tauri signing key, .env, ключи, ssh-keys)
//   - запись файлов >800 строк (правило cohesion из common/coding-style.md)
//
// Exit 2 = блокирующий отказ (Claude Code остановит вызов и покажет сообщение).
// Exit 0 = разрешено.

import { readFileSync } from 'node:fs';

let data = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk) => (data += chunk));
process.stdin.on('end', () => {
  let input;
  try {
    input = JSON.parse(data);
  } catch {
    // Если ввода нет/некорректный — не блокируем.
    process.exit(0);
  }

  const path = (input?.tool_input?.file_path ?? '').toString();

  const FORBIDDEN = [
    { re: /tauri[-_.]?signing[-_.]?private/i, why: 'Tauri minisign приватный ключ (M11.9 паспорта)' },
    { re: /\.minisign$/i, why: 'minisign-ключ' },
    { re: /(^|\/)\.env$/, why: '.env с секретами' },
    { re: /(^|\/)\.env\.(?!example$)[A-Za-z0-9._-]+$/, why: '.env-вариант с секретами' },
    { re: /(^|\/)\.dev\.vars$/, why: '.dev.vars wrangler-секретов' },
    { re: /\.key$/, why: 'private key файл' },
    { re: /\.pem$/, why: 'PEM-сертификат/ключ' },
    { re: /(^|\/)id_(rsa|ed25519|ecdsa|dsa)(\.pub)?$/, why: 'SSH-ключ' },
  ];

  for (const { re, why } of FORBIDDEN) {
    if (re.test(path)) {
      console.error(`[guard-secrets] BLOCKED Write/Edit на: ${path}`);
      console.error(`[guard-secrets] Причина: ${why}`);
      console.error('[guard-secrets] Эти файлы НЕ правятся через агента. Изменения — руками.');
      console.error('[guard-secrets] Если файл легитимен и не секрет — переименуй или удали guard из .claude/settings.json.');
      process.exit(2);
    }
  }

  // [TD-03] Гейт меряет ИТОГОВЫЙ размер файла, а не размер полученной строки.
  // Раньше для Edit считался new_string — то есть фрагмент замены, — поэтому
  // файл можно было наращивать до любого размера серией мелких Edit'ов, ни разу
  // не задев лимит. Для Write итог = content; для Edit = текущий файл ± дельта.
  const lines = resultingLines(path, input?.tool_input);
  if (lines !== null && lines > 800) {
    console.error(`[guard-size] BLOCKED: ${path}`);
    console.error(`[guard-size] Файл станет ${lines} строк > 800 (cohesion limit, common/coding-style.md).`);
    console.error('[guard-size] Раздели на модули. Если намеренно — отредактируй вручную.');
    process.exit(2);
  }

  process.exit(0);
});

function countLines(s) {
  return s.split('\n').length;
}

/** Сколько строк будет в файле ПОСЛЕ применения инструмента. null = не считаем. */
function resultingLines(path, ti) {
  if (typeof ti?.content === 'string') return countLines(ti.content); // Write

  let current;
  try {
    current = countLines(readFileSync(path, 'utf8'));
  } catch {
    return null; // нового файла ещё нет — Edit по нему всё равно упадёт сам
  }

  // MultiEdit: применяем дельты всех правок подряд.
  const edits = Array.isArray(ti?.edits)
    ? ti.edits
    : typeof ti?.new_string === 'string'
      ? [{ old_string: ti.old_string ?? '', new_string: ti.new_string }]
      : [];
  if (edits.length === 0) return null;

  let total = current;
  for (const e of edits) {
    const before = countLines(String(e.old_string ?? ''));
    const after = countLines(String(e.new_string ?? ''));
    total += after - before;
  }
  return total;
}
