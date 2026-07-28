// Абстрактные имена для моделей local-engine, чтобы UI не показывал
// конкретные бренды (Whisper / Qwen / Pyannote) и tech-id. Бренды утекают
// stack-tech-leak'ом и сбивают несведующего юзера. Контракт (Rust
// MODEL_CATALOG.display_name) оставлен как есть — там это нужно для логов и
// дебага. Маппинг хранится в i18n чтобы был переводим.

import type { TranslationKey, useI18n } from '../i18n';

type TFn = ReturnType<typeof useI18n>['t'];

const MODEL_LABEL_KEYS: Record<string, TranslationKey> = {
  'whisper-small': 'localEngine.modelLabel.whisperSmall',
  'whisper-medium': 'localEngine.modelLabel.whisperMedium',
  'whisper-large-v3': 'localEngine.modelLabel.whisperLarge',
  'qwen25-1_5b': 'localEngine.modelLabel.qwenSmall',
  'qwen25-3b': 'localEngine.modelLabel.qwenMedium',
  'qwen25-7b': 'localEngine.modelLabel.qwenLarge',
  'pyannote-segmentation': 'localEngine.modelLabel.diarization',
  // [B22] Каталог вырос — без маппинга в таблице светились сырые id.
  'qwen25-0_5b': 'localEngine.modelLabel.qwenDraft',
  'silero-vad-v5': 'localEngine.modelLabel.vad',
  // [M15.9] Текст-эмбеддер RAG-ассистента (модель + tokenizer.json).
  'e5-small-qint8': 'localEngine.modelLabel.embedder',
  'e5-small-tokenizer': 'localEngine.modelLabel.embedderTokenizer',
  // [B21.6] Голосовой эмбеддер — синтетическая строка хранилища, id вне
  // MODEL_CATALOG (см. utils/voiceStorageRow.ts).
  'voice-embedder': 'localEngine.modelLabel.voiceEmbedder',
};

export function modelLabel(id: string, t: TFn): string {
  const key = MODEL_LABEL_KEYS[id];
  // Fallback на raw id если каталог Rust разъехался с маппингом — лучше
  // показать «whisper-xyz» в UI чем пусто. В норме сюда не доходим:
  // MODEL_CATALOG в Rust и keys тут синхронизированы.
  return key ? t(key) : id;
}
