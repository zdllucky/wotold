// [TD-40] CONTRACTS_VERSION удалена. Константа не потреблялась нигде и не
// бампалась при добавлении summary_v2 / local-engine / assistant — то есть
// создавала ложный сигнал о существующем механизме совместимости, которого
// нет. Реальное версионирование в проекте пер-схемное: `version: 1` внутри
// DiarizedTranscript, `summary_schema_version` на звонке, `latest.json` у
// апдейтера. Если механизм понадобится — заводить его вместе с потребителем
// (например handshake MCP), а не константой впрок.

export * from './transcript.js';
export * from './recap.js';
export * from './summary_v2.js';
export * from './updater.js';
export * from './local-engine.js';
export * from './assistant.js';
export * from './degraded.js';
