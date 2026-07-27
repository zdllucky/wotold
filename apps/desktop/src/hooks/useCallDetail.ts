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

import { useEffect, useRef, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import {
  getCall,
  getCallAudioPath,
  isCallProcessing,
  listCallActionItems,
  listCallDecisions,
  listCallOpenQuestions,
  readCallArtifact,
  type ActionItem,
  type Decision,
  type OpenQuestion,
  type PipelineCancelledEvent,
} from '../api/calls';
import { listContacts, type Contact } from '../api/contacts';
import {
  listCallSpeakers,
  type CallAutoBoundEvent,
  type CallSpeakerView,
} from '../api/speakers';
import {
  listCallChunks,
  RECAP_PROGRESS_EVENT,
  RECAP_STEP_EVENT,
  RECORDING_DURATION_EVENT,
  type Call,
  type CallChunk,
  type CallProgressEvent,
  type ChunkDoneEvent,
  type RecapProgressEvent,
  type RecapStepEvent,
  type RecordingDurationEvent,
} from '../api/recording';
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
  /** [M13.3.1] Chunks для chunked-pipeline записей. Пустой массив для
   *  cloud-managed / legacy local — ProcessingPanel fallback'нет на 5-step
   *  PipelineStrip. */
  chunks: CallChunk[];
  /** [M14 T-11] V2 structured summary blocks. Пустой массив для legacy
   *  schema_version=1 либо если LLM не вернул decisions. */
  decisions: Decision[];
  openQuestions: OpenQuestion[];
  micSrc: string | null;
  systemSrc: string | null;
  /** [P1.3] Elapsed seconds во время local LLM recap regen. `null` если нет
   *  активной generation либо cloud engine (cloud не emit'ит этот event).
   *  Resets на `null` когда `regenerating` flips false в CallDetailPage. */
  recapElapsedSec: number | null;
  setRecapElapsedSec: (v: number | null) => void;
  /** [F3] Живые шаги генерации рекапа (`recap:step`, upsert по step_idx).
   *  Очищается насовсем при смене звонка и на pipeline:finished/cancelled —
   *  thinking-блок существует только во время генерации. */
  recapSteps: RecapStepEvent[];
  /** [Global regen] Активна ли фон-задача (reprocess / regen) для звонка —
   *  из backend pipeline_tasks registry, переживает навигацию. */
  bgBusy: boolean;
  setBgBusy: (v: boolean) => void;
  /** [P-fix10] One-shot: пайплайн только что завершился → recap/transcript
   *  «печатаются» (typewriter/каскад). Авто-сброс ~6s + при смене звонка,
   *  чтобы обычная навигация не реанимировала. */
  justGenerated: boolean;
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
  const [chunks, setChunks] = useState<CallChunk[]>([]);
  const [decisions, setDecisions] = useState<Decision[]>([]);
  const [openQuestions, setOpenQuestions] = useState<OpenQuestion[]>([]);
  const [micSrc, setMicSrc] = useState<string | null>(null);
  const [systemSrc, setSystemSrc] = useState<string | null>(null);
  // [P1.3] Elapsed timer для recap regen UI. См. JSDoc на интерфейсе.
  const [recapElapsedSec, setRecapElapsedSec] = useState<number | null>(null);
  // [F3] Шаги thinking-блока генерации рекапа.
  const [recapSteps, setRecapSteps] = useState<RecapStepEvent[]>([]);
  // [Global regen] Активна ли фон-задача (reprocess / regen) для звонка.
  // Источник правды — backend pipeline_tasks registry (через isCallProcessing
  // + pipeline:started/finished события), а не component-local флаг → переживает
  // уход со страницы и возврат.
  const [bgBusy, setBgBusy] = useState(false);
  const [justGenerated, setJustGenerated] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // [P-fix10] Сброс one-shot reveal-флага при смене звонка — чтобы переход на
  // другой (уже готовый) звонок не запускал typewriter.
  // [F3] + очистка thinking-шагов: блок не пересекает границы звонков.
  useEffect(() => {
    setJustGenerated(false);
    setRecapSteps([]);
  }, [callId]);

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
    // [TD-24] Флаг отмены — как в useCallAudio. Без него resolve запроса по
    // старому звонку перезаписывал данные уже открытого нового: 12 ресурсов
    // резолвятся вразнобой, а страница до этого монтировалась без key.
    let cancelled = false;
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
      listCallChunks(callId),
      listCallDecisions(callId),
      listCallOpenQuestions(callId),
    ])
      .then(
        ([
          rCall,
          rRecap,
          rTrans,
          rRaw,
          rTasks,
          rContacts,
          rSpeakers,
          rMic,
          rSys,
          rChunks,
          rDecisions,
          rOpenQ,
        ]) => {
          // [TD-24] Звонок уже сменился — молча уходим, чужие данные в
          // состояние текущего не пишем.
          if (cancelled) return;
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
          if (rChunks.status === 'fulfilled') setChunks(rChunks.value);
          if (rDecisions.status === 'fulfilled') setDecisions(rDecisions.value);
          if (rOpenQ.status === 'fulfilled') setOpenQuestions(rOpenQ.value);
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
            ['chunks', rChunks],
            ['decisions', rDecisions],
            ['open_questions', rOpenQ],
          ] as const) {
            if (r.status === 'rejected')
              console.warn(`CallDetail ${name} load failed`, r.reason);
          }
        },
      )
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
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
    // [Global regen] Restore busy-состояния после возврата на страницу: если
    // фон-задача (reprocess / regen) ещё бежит — bgBusy=true.
    void isCallProcessing(callId)
      .then(setBgBusy)
      .catch(() => {});
    const unlisteners: UnlistenFn[] = [];
    const attach = async () => {
      try {
        const u1 = await listen<{ call_id: string }>('pipeline:started', (e) => {
          if (e.payload.call_id !== callId) return;
          setBgBusy(true);
          // Status уже processing — лёгкий refetch только meta для синка.
          void getCall(callId).then((c) => c && setCallState(c));
        });
        unlisteners.push(u1);
        const u2 = await listen<{ call_id: string; status: string }>(
          'pipeline:finished',
          (e) => {
            if (e.payload.call_id !== callId) return;
            setBgBusy(false);
            // [F3] Генерация закончилась → thinking-блок исчезает насовсем.
            setRecapSteps([]);
            // [P-fix10] One-shot «только что сгенерировано» → typewriter-reveal.
            // Авто-сброс позже длительности анимации, чтобы навигация не реанимировала.
            setJustGenerated(true);
            window.setTimeout(() => setJustGenerated(false), 6000);
            void refetchAll();
          },
        );
        unlisteners.push(u2);
        const u3 = await listen<PipelineCancelledEvent>(
          'pipeline:cancelled',
          (e) => {
            if (e.payload.call_id !== callId) return;
            setBgBusy(false);
            // [F3] Отмена — шаги тоже убираем.
            setRecapSteps([]);
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

  // [M13.3.1] Live chunk progress — слушаем `transcript:chunk_done` события
  // для этого звонка. Status текущей row патчится; для new chunk_idx (или если
  // массив пустой при первом emit) — refetch listCallChunks (lightweight,
  // ≤24 chunks типичный max).
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<ChunkDoneEvent>('transcript:chunk_done', (e) => {
      if (e.payload.call_id !== callId) return;
      const { chunk_idx, status } = e.payload;
      setChunks((prev) => {
        const idx = prev.findIndex((c) => c.chunk_idx === chunk_idx);
        const target = idx === -1 ? undefined : prev[idx];
        if (!target) {
          // New chunk_idx (или начальный массив пуст) — refetch.
          void listCallChunks(callId)
            .then(setChunks)
            .catch((err) => console.warn('refetch chunks failed', err));
          return prev;
        }
        const next = prev.slice();
        next[idx] = { ...target, status };
        return next;
      });
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => console.warn('transcript:chunk_done listener:', err));
    return () => unlisten?.();
  }, [callId]);

  // [P5.2] Live duration update во время active recording. Patches
  // `call.duration_sec` на каждый sidecar `rotated` event (~раз в 10 мин).
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<RecordingDurationEvent>(RECORDING_DURATION_EVENT, (e) => {
      if (e.payload.call_id !== callId) return;
      setCallState((prev) =>
        prev ? { ...prev, duration_sec: e.payload.duration_sec } : prev,
      );
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => console.warn('recording:duration listener:', err));
    return () => unlisten?.();
  }, [callId]);

  // [P1.3] Live elapsed timer для local LLM recap generation. Backend
  // `pipeline::recap_progress::with_recap_progress_emitter` шлёт каждые 15s
  // пока future не resolve'нется. UI рендерит «Пересоздаём… {sec}s» в
  // HeaderActions. Reset делает CallDetailPage когда `regenerating` flips
  // false (в onRegenerateRecap finally).
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<RecapProgressEvent>(RECAP_PROGRESS_EVENT, (e) => {
      if (e.payload.call_id !== callId) return;
      setRecapElapsedSec(e.payload.elapsed_sec);
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => console.warn('recap:progress listener:', err));
    return () => unlisten?.();
  }, [callId]);

  // [F3] Живые шаги генерации рекапа — upsert по step_idx (started → done
  // обновляет ту же запись, порядок стабилен). Очистка: смена звонка,
  // pipeline:finished, pipeline:cancelled (см. выше).
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<RecapStepEvent>(RECAP_STEP_EVENT, (e) => {
      if (e.payload.call_id !== callId) return;
      const ev = e.payload;
      setRecapSteps((prev) => {
        const idx = prev.findIndex((s) => s.step_idx === ev.step_idx);
        if (idx === -1) {
          return [...prev, ev].sort((a, b) => a.step_idx - b.step_idx);
        }
        const next = prev.slice();
        next[idx] = ev;
        return next;
      });
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => console.warn('recap:step listener:', err));
    return () => unlisten?.();
  }, [callId]);

  // [V6.4 + Bug-fix #5] Live pipeline progress — слушаем `call:progress`
  // события для этого звонка и патчим Call object. Дополнительно: на
  // переход stage'а (e.payload.step != prevStep) триггерим debounced
  // refetchAll'у, чтобы UI подтягивал свежие артефакты (transcript /
  // raw_stt / recap) по мере их появления, без необходимости выходить
  // и заходить в звонок заново. Debounce — 600ms на step transition,
  // rate-limit — не чаще раз в 1.5s.
  const prevStepRef = useRef<number | null>(null);
  const refetchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastRefetchAtRef = useRef<number>(0);
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    const scheduleRefetch = () => {
      const now = Date.now();
      const sinceLast = now - lastRefetchAtRef.current;
      const delay = Math.max(600, 1500 - sinceLast);
      if (refetchTimerRef.current) clearTimeout(refetchTimerRef.current);
      refetchTimerRef.current = setTimeout(() => {
        lastRefetchAtRef.current = Date.now();
        void refetchAll();
      }, delay);
    };
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
      if (e.payload.step !== prevStepRef.current) {
        prevStepRef.current = e.payload.step;
        scheduleRefetch();
      }
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => console.warn('call:progress listener:', err));
    return () => {
      unlisten?.();
      if (refetchTimerRef.current) {
        clearTimeout(refetchTimerRef.current);
        refetchTimerRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- refetchAll stable per callId
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
  // [M13.3.1] + chunks (для chunked-pipeline записей; пустой для cloud/legacy).
  const refetchAll = async () => {
    const [
      fresh,
      freshTranscript,
      freshRaw,
      freshTasks,
      freshCall,
      freshSpeakers,
      freshChunks,
      freshDecisions,
      freshOpenQ,
    ] = await Promise.allSettled([
      readCallArtifact(callId, 'recap'),
      readCallArtifact(callId, 'transcript'),
      readCallArtifact(callId, 'raw_stt'),
      listCallActionItems(callId),
      getCall(callId),
      listCallSpeakers(callId),
      listCallChunks(callId),
      listCallDecisions(callId),
      listCallOpenQuestions(callId),
    ]);
    if (fresh.status === 'fulfilled') setRecap(fresh.value);
    if (freshTranscript.status === 'fulfilled') setTranscript(freshTranscript.value);
    if (freshRaw.status === 'fulfilled') setRawStt(freshRaw.value);
    if (freshTasks.status === 'fulfilled') setTasks(freshTasks.value);
    if (freshCall.status === 'fulfilled') setCallState(freshCall.value);
    if (freshSpeakers.status === 'fulfilled') setSpeakers(freshSpeakers.value);
    if (freshChunks.status === 'fulfilled') setChunks(freshChunks.value);
    if (freshDecisions.status === 'fulfilled') setDecisions(freshDecisions.value);
    if (freshOpenQ.status === 'fulfilled') setOpenQuestions(freshOpenQ.value);
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
    chunks,
    decisions,
    openQuestions,
    micSrc,
    systemSrc,
    recapElapsedSec,
    setRecapElapsedSec,
    recapSteps,
    bgBusy,
    setBgBusy,
    justGenerated,
    loading,
    error,
    setError,
    refetchAll,
    refetchSpeakersAndContacts,
  };
}
