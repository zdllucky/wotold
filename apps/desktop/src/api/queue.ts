// [Q] Очереди тяжёлых local-ресурсов (whisper STT / диаризация / LLM).
// Backend: pipeline/resource_queue.rs — permit=1 на ресурс, FIFO; на каждый
// transition эмитится полный снапшот `queue:state`. Initial state — команда
// `get_queue_state`.

import { invoke } from '@tauri-apps/api/core';

export type QueueResourceId = 'stt' | 'diarization' | 'llm';

/** Запись в очереди/работе. `call_id=null` — служебная задача (warm-up). */
export interface QueueTicket {
  call_id: string | null;
}

export interface QueueResourceState {
  id: QueueResourceId;
  busy: QueueTicket | null;
  /** FIFO; позиция = index + 1. Один звонок может встретиться дважды
   *  (mic+system дорожки STT) — UI дедуплицирует. */
  waiting: QueueTicket[];
}

export interface QueueState {
  resources: QueueResourceState[];
}

export const QUEUE_STATE_EVENT = 'queue:state';

export function getQueueState(): Promise<QueueState> {
  return invoke<QueueState>('get_queue_state');
}
