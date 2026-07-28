# MCP connector

Гайд по подключению переехал на сайт и живёт в трёх локалях:

**<https://zdllucky.github.io/wotold/mcp/>**

Исходник — [`apps/site/src/content/docs/mcp.md`](../apps/site/src/content/docs/mcp.md).
Правки вносятся туда.

Что осталось здесь — указатели на код, а не на документацию:

- Реализация сервера и инструментов: [`services/mcp/src/tools.ts`](../services/mcp/src/tools.ts)
- Требования паспорта: M8.2 (набор инструментов), M8.3 (контент звонков как недоверенные данные), M8.4 (никаких сетевых вызовов)
- Отклонение от M8.2: `find_calls_by_contact` принимает имя контакта, а не id — обоснование в тексте на сайте
- Security-триггер: любое изменение в `services/mcp/**` требует прогона `/security-scan` до merge (CLAUDE.md, раздел про security-review)
