// [Bug-fix] Engine label helper — превращает `Call.summary_engine` (raw
// строка из backend) в i18n-readable обозначение для UI диагностики.
//
// Backend values (см. pipeline/recap.rs::persist_recap_from_json вызовы):
// - "cloud-managed"     — Anthropic Sonnet через Wotold proxy
// - "local-qwen-1.5b"   — local Light preset
// - "local-qwen-3b"     — local Balanced
// - "local-qwen-7b"     — local Quality
// - "local-qwen"        — generic fallback
//
// Возвращает короткий human label, fallback на raw value (для unknown engines
// сохраняем оригинал — debug сигнал что backend добавил новый label).

export interface EngineLabelStrings {
  cloud: string;
  localLight: string;
  localBalanced: string;
  localQuality: string;
  localGeneric: string;
}

export function engineLabelHuman(
  engine: string | null | undefined,
  strings: EngineLabelStrings,
): string | null {
  if (!engine) return null;
  switch (engine) {
    case 'cloud-managed':
      return strings.cloud;
    case 'local-qwen-1.5b':
      return strings.localLight;
    case 'local-qwen-3b':
      return strings.localBalanced;
    case 'local-qwen-7b':
      return strings.localQuality;
    case 'local-qwen':
      return strings.localGeneric;
    default:
      return engine;
  }
}
