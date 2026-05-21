// [Phase 5 R7] State machine for CallDetailPage — отделяет load + event
// listeners + refetch actions от чисто-JSX страницы. Сохраняет 1-в-1 поведение
// прежнего внутри-страничного state (4 useEffect, optimistic patches on
// reprocess не входят сюда — это action из самой страницы).
//
// Owns:
// - initial Promise.allSettled load: call meta + 3 artifacts + tasks + contacts
//   + speakers + 2 audio paths (= 9 ресурсов).
// - listen'ы 4-х tauri events: pipeline:started / pipeline:finished /
//   pipeline:cancelled / call:progress / call:auto_bound.
// - refetchAll() — после reprocess/regenerate-recap или pipeline:finished.
// - refetchSpeakersAndContacts() — после confirm/unbind speaker.
//
// **НЕ** owns:
// - tab state, kebab menu open state, busy flags (deleting/reprocessing/…)
//   — это UI-only, остаётся в самом CallDetailPage.

import { useEffect, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import {
  getCall,
  getCallAudioPath,
  listCallActionItems,
  readCallArtifact,
  type ActionItem,
  type PipelineCancelledEvent,
} from '../api/calls';
import { listContacts, type Contact } from '../api/contacts';
import {
  listCallSpeakers,
  type CallAutoBoundEvent,
  type CallSpeakerView,
} from '../api/speakers';
import type { Call, CallProgressEvent } from '../api/recording';
import { humanError } from '../api/errors';

export interface UseCallDetailResult {
  call: Call | null;
  setCall: (updater: Call | ((prev: Call | null) => Call | null)) => void;
  recap: string | null;
  setRecap: (v: string | null) => void;
  transcript: string | null;
  rawStt: string | null;
  tasks: ActionItem[] | null;
  setTasks: (v: ActionItem[] | null) => void;
  contacts: Contact[];
  speakers: CallSpeakerView[];
  micSrc: string | null;
  systemSrc: string | null;
  loading: boolean;
  error: string | null;
  setError: (v: string | null) => void;
  refetchAll: () => Promise<void>;
  refetchSpeakersAndContacts: () => Promise<void>;
}

export function useCallDetail(callId: string): UseCallDetailResult {
  const [call, setCallState] = useState<Call | null>(null);
  const [recap, setRecap] = useState<string | null>(null);
  const [transcript, setTranscript] = useState<string | null>(null);
  const [rawStt, setRawStt] = useState<string | null>(null);
  const [tasks, setTasks] = useState<ActionItem[] | null>(null);
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [speakers, setSpeakers] = useState<CallSpeakerView[]>([]);
  const [micSrc, setMicSrc] = useState<string | null>(null);
  const [systemSrc, setSystemSrc] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Adapter that supports either a value or an updater callback — mirrors
  // React's useState setter API so consumer can do
  // `setCall((prev) => ({...prev, status: 'processing'}))` exactly как раньше.
  const setCall = (updater: Call | ((prev: Call | null) => Call | null)) => {
    if (typeof updater === 'function') {
      setCallState(updater as (prev: Call | null) => Call | null);
    } else {
      setCallState(updater);
    }
  };

  // Initial load — allSettled чтобы failed artifact (например ещё не существует
  // recap.md когда юзер открыл звонок до завершения pipeline) не ломал все
  // state setters.
  useEffect(() => {
    setLoading(true);
    setError(null);
    Promise.allSettled([
      getCall(callId),
      readCallArtifact(callId, 'recap'),
      readCallArtifact(callId, 'transcript'),
      readCallArtifact(callId, 'raw_stt'),
      listCallActionItems(callId),
      listContacts(),
      listCallSpeakers(callId),
      getCallAudioPath(callId, 'mic'),
      getCallAudioPath(callId, 'system'),
    ])
      .then(([rCall, rRecap, rTrans, rRaw, rTasks, rContacts, rSpeakers, rMic, rSys]) => {
        // Call meta — критично. Без неё страница не имеет смысла.
        if (rCall.status === 'fulfilled') {
          setCallState(rCall.value);
        } else {
          setError(humanError(rCall.reason));
        }
        if (rRecap.status === 'fulfilled') setRecap(rRecap.value);
        if (rTrans.status === 'fulfilled') setTranscript(rTrans.value);
        if (rRaw.status === 'fulfilled') setRawStt(rRaw.value);
        if (rTasks.status === 'fulfilled') setTasks(rTasks.value);
        if (rContacts.status === 'fulfilled') setContacts(rContacts.value);
        if (rSpeakers.status === 'fulfilled') setSpeakers(rSpeakers.value);
        setMicSrc(rMic.status === 'fulfilled' ? convertFileSrc(rMic.value) : null);
        setSystemSrc(rSys.status === 'fulfilled' ? convertFileSrc(rSys.value) : null);
        // Log невидимые failures чтобы они не исчезли silent.
        for (const [name, r] of [
          ['recap', rRecap],
          ['transcript', rTrans],
          ['raw_stt', rRaw],
          ['tasks', rTasks],
          ['contacts', rContacts],
          ['speakers', rSpeakers],
        ] as const) {
          if (r.status === 'rejected') console.warn(`CallDetail ${name} load failed`, r.reason);
        }
      })
      .finally(() => setLoading(false));
  }, [callId]);

  // [V7] auto-bound event — pipeline закончил matching и нашёл N speaker'ов
  // с score >= threshold. Refetch speakers и показываем undo баннер.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<CallAutoBoundEvent>('call:auto_bound', (e) => {
      if (e.payload.call_id !== callId) return;
      void refetchSpeakersAndContacts();
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => console.warn('call:auto_bound listener:', err));
    return () => unlisten?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- refetch handle stable across renders
  }, [callId]);

  // [V8] Lifecycle events — pipeline:started / pipeline:finished /
  // pipeline:cancelled триггерят refetch'и для текущего звонка.
  // На started: refetch call meta (статус 'processing' уже стоит, ок).
  // На finished/cancelled: refetch всё (артефакты могли обновиться).
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    const attach = async () => {
      try {
        const u1 = await listen<{ call_id: string }>('pipeline:started', (e) => {
          if (e.payload.call_id !== callId) return;
          // Status уже processing — лёгкий refetch только meta для синка.
          void getCall(callId).then((c) => c && setCallState(c));
        });
        unlisteners.push(u1);
        const u2 = await listen<{ call_id: string; status: string }>(
          'pipeline:finished',
          (e) => {
            if (e.payload.call_id !== callId) return;
            void refetchAll();
          },
        );
        unlisteners.push(u2);
        const u3 = await listen<PipelineCancelledEvent>(
          'pipeline:cancelled',
          (e) => {
            if (e.payload.call_id !== callId) return;
            void refetchAll();
          },
        );
        unlisteners.push(u3);
      } catch (e) {
        console.warn('pipeline lifecycle listeners failed:', e);
      }
    };
    void attach();
    return () => {
      for (const u of unlisteners) u();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- refetchAll stable per callId
  }, [callId]);

  // [V6.4] Live pipeline progress — слушаем `call:progress` события для этого
  // звонка и патчим Call object. UI: PipelineStrip + reassurance banner.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<CallProgressEvent>('call:progress', (e) => {
      if (e.payload.call_id !== callId) return;
      setCallState((prev) =>
        prev
          ? {
              ...prev,
              pipeline_step: e.payload.step,
              pipeline_pct: e.payload.pct,
              pipeline_eta_sec: e.payload.eta_sec,
              upload_bytes: e.payload.upload_bytes,
            }
          : prev,
      );
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => console.warn('call:progress listener:', err));
    return () => unlisten?.();
  }, [callId]);

  // [V4.1] Перечитать speakers + contacts после mutation в табе или
  // inline modal. SpeakersSection + SpeakerConfirmModal вызывают это
  // → ParticipantsRow + chip'ы в транскрипте обновятся динамически.
  const refetchSpeakersAndContacts = async () => {
    try {
      const [s, c] = await Promise.all([
        listCallSpeakers(callId),
        listContacts(),
      ]);
      setSpeakers(s);
      setContacts(c);
    } catch (e) {
      console.warn('refetch speakers/contacts failed', e);
    }
  };

  // [V8] Refetch full state — артефакты + call meta + speakers. Используется
  // и в reprocess, и в pipeline:finished/cancelled listeners.
  const refetchAll = async () => {
    const [fresh, freshTranscript, freshRaw, freshTasks, freshCall, freshSpeakers] =
      await Promise.allSettled([
        readCallArtifact(callId, 'recap'),
        readCallArtifact(callId, 'transcript'),
        readCallArtifact(callId, 'raw_stt'),
        listCallActionItems(callId),
        getCall(callId),
        listCallSpeakers(callId),
      ]);
    if (fresh.status === 'fulfilled') setRecap(fresh.value);
    if (freshTranscript.status === 'fulfilled') setTranscript(freshTranscript.value);
    if (freshRaw.status === 'fulfilled') setRawStt(freshRaw.value);
    if (freshTasks.status === 'fulfilled') setTasks(freshTasks.value);
    if (freshCall.status === 'fulfilled') setCallState(freshCall.value);
    if (freshSpeakers.status === 'fulfilled') setSpeakers(freshSpeakers.value);
  };

  return {
    call,
    setCall,
    recap,
    setRecap,
    transcript,
    rawStt,
    tasks,
    setTasks,
    contacts,
    speakers,
    micSrc,
    systemSrc,
    loading,
    error,
    setError,
    refetchAll,
    refetchSpeakersAndContacts,
  };
}
