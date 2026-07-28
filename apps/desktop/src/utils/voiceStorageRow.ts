// [B21.6] Строка WeSpeaker-эмбеддера для таблицы «Хранилище моделей».
//
// Голосовой эмбеддер живёт отдельно от MODEL_CATALOG (своя команда
// voice_model_*, свой файл models/embedder.onnx), поэтому в
// `local_engine_storage_list` его нет. В таблице хранилища это выглядело как
// пропажа: 25 МБ на диске, которых нет ни в одной строке, и удалить их можно
// было только из другого раздела настроек. Приводим его статус к форме
// каталога и подмешиваем строкой — вызовы download/delete для этого id
// маршрутизируются на voice_model_* в LocalEngineSection.

import type { LocalEngineStorageRow } from '../api/local-engine';
import type { VoiceModelStatus } from '../api/voiceModel';

/** Синтетический id: в MODEL_CATALOG его нет, поэтому нужен свой маршрут. */
export const VOICE_EMBEDDER_ROW_ID = 'voice-embedder';

export function voiceEmbedderRow(
  status: VoiceModelStatus,
  sizeHint: number,
): LocalEngineStorageRow {
  const id = VOICE_EMBEDDER_ROW_ID;
  const size = status.status === 'missing' ? sizeHint : status.size;
  return {
    id,
    kind: 'diarization',
    display_name: 'WeSpeaker ResNet34-LM',
    size_bytes: size,
    status:
      status.status === 'valid'
        ? { state: 'present', id, bytes_total: size }
        : status.status === 'corrupted'
          ? {
              state: 'corrupted',
              id,
              bytes_done: size,
              bytes_total: size,
              expected: status.expected,
              got: status.got,
            }
          : { state: 'absent', id, bytes_total: size },
    last_used_at: null,
    // Эмбеддер не входит ни в один preset — «активной» строкой не считается,
    // иначе удаление уводило бы в модалку смены пресета, которой тут нет.
    is_active: false,
  };
}
