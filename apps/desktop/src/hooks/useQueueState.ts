// [Q] Глобальное состояние очередей ресурсов: initial snapshot через команду
// + live-обновления по `queue:state` (паттерн pipeline:* listeners в App.tsx).

import { useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { getQueueState, QUEUE_STATE_EVENT, type QueueState } from '../api/queue';

export function useQueueState(): QueueState | null {
  const [queue, setQueue] = useState<QueueState | null>(null);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let alive = true;
    getQueueState()
      .then((q) => {
        if (alive) setQueue(q);
      })
      .catch((e) => console.warn('get_queue_state failed:', e));
    listen<QueueState>(QUEUE_STATE_EVENT, (e) => {
      setQueue(e.payload);
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((e) => console.warn('queue:state listener:', e));
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  return queue;
}
