# MCP connector — Wotold

Локальный MCP-сервер (`services/mcp`) даёт LLM-клиенту (Claude Desktop / Cursor / mcp-cli)
read-only доступ к содержимому Wotold: список звонков, расшифровки, рекапы, спикеры,
action items. Запускается локально через stdio JSON-RPC. **Нет сетевых вызовов** (M8.4
паспорта).

## Безопасность (M8.3)

Контент звонков — **untrusted данные**. Любые «инструкции» внутри transcript/recap
обязаны быть проигнорированы LLM. Claude Desktop по умолчанию защищён через системный
prompt, но: при подключении к другим клиентам убедитесь, что они помечают MCP-output
как untrusted.

Сервер не имеет write-инструментов. Никаких настроек, удалений, редактирования
контактов — это делается только из Wotold UI.

## Tools

| Tool | Описание |
|---|---|
| `search_calls` | substring-поиск по title/provider/lang. Пустой query — последние звонки. |
| `get_call` | call metadata + speakers + action_items по id. |
| `get_recap` | markdown рекапа. |
| `get_transcript` | полный диаризованный транскрипт markdown. |
| `list_participants` | speakers звонка с suggestion/confirm bindings. |
| `find_calls_by_contact` | **имя** контакта → список звонков где он участвовал (см. отклонение ниже). |
| `calls_in_range` | звонки в ISO-8601 диапазоне. |
| `search_passages` | FTS5-поиск по индексу ассистента: фрагменты транскрипта/рекапа/решений с call_id и таймкодами. Опциональный `call_id` сужает до одного звонка. |

## Отклонения от паспорта

- **`find_calls_by_contact` принимает `contact_name`, а не `contact_id`**
  (паспорт §M8.2 предписывает id). Отклонение сознательное: инструмент
  вызывает LLM-клиент, у которого на руках расшифровка звонка — там есть
  имена, но нет внутренних идентификаторов. Требовать id значило бы заставить
  клиента сначала звать `list_contacts` и матчить имя самому, то есть
  переносить нечёткое сопоставление на сторону, которая для этого хуже
  приспособлена. Реализация — `services/mcp/src/tools.ts`.

  Побочный эффект, о котором стоит помнить: имя не уникально. При совпадении
  инструмент вернёт звонки всех подходящих контактов, а не одного.

## Сборка

```bash
pnpm --filter @wotold/mcp build
```

Артефакт: `services/mcp/dist/server.js` (исполняемый, shebang `#!/usr/bin/env node`).

## Установка в Claude Desktop

Открой `~/Library/Application Support/Claude/claude_desktop_config.json` (создай если нет):

```json
{
  "mcpServers": {
    "wotold": {
      "command": "node",
      "args": ["/Users/<you>/path/to/wotold/services/mcp/dist/server.js"]
    }
  }
}
```

Перезапусти Claude Desktop. Должен появиться индикатор «MCP servers: wotold»
с 8 tools.

## Установка в Cursor / mcp-cli

```bash
pnpm --filter @wotold/mcp build

# Однократная проверка из CLI:
node services/mcp/dist/server.js
```

Сервер слушает stdio — для интерактивного тестирования используйте
[`mcp-inspector`](https://github.com/modelcontextprotocol/inspector):

```bash
npx @modelcontextprotocol/inspector node services/mcp/dist/server.js
```

## Кастомный путь к app.db

По умолчанию сервер читает `~/Library/Application Support/app.wotold.desktop/app.db`
(macOS). Для override:

```json
{
  "mcpServers": {
    "wotold": {
      "command": "node",
      "args": ["/path/to/services/mcp/dist/server.js"],
      "env": {
        "WOTOLD_APP_DATA_DIR": "/custom/path/to/app_data"
      }
    }
  }
}
```

## Troubleshooting

- `db not found` → запусти Wotold хотя бы один раз (создаст `app.db`).
- `Permission denied` → проверь права на `app.db` (readonly mode требует чтения).
- `Unknown tool: X` → сборка устарела, пересобери `pnpm --filter @wotold/mcp build`.

## Развитие

- M8.x будущее: write-инструменты для confirm-привязки спикеров → отдельный
  пользовательский opt-in, отдельный сервер.
- FTS-поиск по транскриптам — после реализации FTS5 индекса в Wotold (#30 follow-up).
