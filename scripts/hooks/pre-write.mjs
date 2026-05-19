#!/usr/bin/env node
// PreToolUse hook для Write/Edit.
// Блокирует:
//   - попытки правки файлов с секретами (Tauri signing key, .env, ключи, ssh-keys)
//   - запись файлов >800 строк (правило cohesion из common/coding-style.md)
//
// Exit 2 = блокирующий отказ (Claude Code остановит вызов и покажет сообщение).
// Exit 0 = разрешено.

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
  const content = (input?.tool_input?.content ?? input?.tool_input?.new_string ?? '').toString();

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

  if (content) {
    const lines = content.split('\n').length;
    if (lines > 800) {
      console.error(`[guard-size] BLOCKED: ${path}`);
      console.error(`[guard-size] Файл ${lines} строк > 800 (cohesion limit, common/coding-style.md).`);
      console.error('[guard-size] Раздели на модули. Если намеренно — отредактируй вручную.');
      process.exit(2);
    }
  }

  process.exit(0);
});
