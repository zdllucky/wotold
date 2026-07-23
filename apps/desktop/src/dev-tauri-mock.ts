// Dev-only mock for Tauri invoke API when running plain Vite in browser (no webview).
// Lets us preview pages visually without spinning up `tauri dev`. Inert in production.

import type { AssistantAnswer, AssistantMessage } from '@wotold/contracts';

declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      invoke: (cmd: string, args?: unknown) => Promise<unknown>;
      [key: string]: unknown;
    };
  }
}

if (import.meta.env.DEV && !window.__TAURI_INTERNALS__) {
  const ownerContact = {
    id: '00000000-0000-0000-0000-000000000001',
    display_name: 'Дамир',
    is_owner: true,
    org: null,
    role: null,
    notes: null,
    identifiers: [{ id: 'i1', kind: 'phone', value: '+7 777 000 00 00' }],
    attributes: { linkedin: 'damir' },
    created_at: '2025-01-01T00:00:00Z',
  };

  const sampleContact = {
    id: '00000000-0000-0000-0000-000000000002',
    display_name: 'Иван Петров',
    is_owner: false,
    org: 'Acme Corp',
    role: 'CTO',
    notes: 'Партнёр по KZT-инвойсу',
    identifiers: [
      { id: 'i2', kind: 'email', value: 'ivan@acme.kz' },
      { id: 'i3', kind: 'phone', value: '+7 701 234 56 78' },
    ],
    attributes: {},
    created_at: '2025-02-01T00:00:00Z',
  };

  const sampleCalls = [
    {
      id: 'aaaaaaaa-1111-1111-1111-111111111111',
      title: 'Звонок с Acme',
      started_at: '2026-05-19T14:23:00Z',
      stopped_at: '2026-05-19T14:38:00Z',
      duration_sec: 900,
      status: 'ready',
      path_label: '~/Wotold/2026-05-19',
      provider: 'soniox',
      lang_detected: 'ru',
    },
    {
      id: 'bbbbbbbb-2222-2222-2222-222222222222',
      title: null,
      started_at: '2026-05-20T08:10:00Z',
      stopped_at: null,
      duration_sec: 180,
      status: 'processing',
      path_label: '~/Wotold/2026-05-20',
      provider: null,
      lang_detected: null,
    },
    {
      id: 'cccccccc-3333-3333-3333-333333333333',
      title: 'Старый звонок',
      started_at: '2026-05-15T10:00:00Z',
      stopped_at: '2026-05-15T10:05:00Z',
      duration_sec: 305,
      status: 'failed',
      path_label: '~/Wotold/2026-05-15',
      provider: 'gladia',
      lang_detected: null,
      failed_reason:
        'Квота STT исчерпана. Подожди до следующих суток или переключись на BYO.',
      recap_failed_reason: null,
    },
  ];

  const responses: Record<string, unknown> = {
    get_device_id: 'dev-mock-device-id-0001',
    check_for_update: null,
    list_contacts: [ownerContact, sampleContact],
    list_calls: sampleCalls,
    get_recording_state: null,
    get_audio_permissions: { microphone: 'granted', screen_recording: 'not_determined' },
  };

  // #47: in-memory BYO key store (dev only — production использует Keychain).
  const byoKeys: Record<string, string> = {};

  const initialOnboarding =
    new URLSearchParams(window.location.search).get('onboarding') === '1' ? '0' : '1';
  const settings: Record<string, string> = {
    onboarding_done: initialOnboarding,
    stt_provider: 'auto',
    provider_path: 'managed',
    llm_model: 'claude-sonnet-4-6',
    proxy_base_url: 'http://dev-proxy.local',
  };

  // #38: in-memory session store для AccountSection preview.
  let accountSessionToken: string | null = null;

  // #26: in-memory speaker confirmations для preview. ключ "<callId>:<S-tag>".
  const speakerBindings: Record<string, string> = {};

  // #45: in-memory voice samples preview.
  const voiceSamples: Array<{
    id: string;
    contact_id: string;
    source_call: string | null;
    quality: number | null;
    created_at: string;
    embedding_bytes: number;
  }> = [
    {
      id: 'vs-mock-1',
      contact_id: sampleContact.id,
      source_call: sampleCalls[0]?.id ?? null,
      quality: 0.87,
      created_at: '2026-05-10T14:30:00Z',
      embedding_bytes: 1024,
    },
    {
      id: 'vs-mock-2',
      contact_id: sampleContact.id,
      source_call: sampleCalls[1]?.id ?? null,
      quality: 0.92,
      created_at: '2026-05-15T09:12:00Z',
      embedding_bytes: 1024,
    },
  ];

  // ── [B24.2] Ассистент: in-memory store + движок-мок (банк из хендоффа).
  // Типы — из контракта (@wotold/contracts): дрейф формы ловит tsc (ревью M4).
  interface MockAssistantChat {
    chat: { id: string; callId: string | null; title: string; createdAt: string; updatedAt: string };
    messages: AssistantMessage[];
    /** Для сортировки списка как в проде: ORDER BY updated_at DESC (ревью M2). */
    updatedAt: number;
  }
  const assistantChats: MockAssistantChat[] = [];
  // [B25] In-memory состояние тумблера семантического поиска (default on).
  let mockSemanticSearch = true;
  let assistantSeq = 0;
  const AS_GEN_RE = /напиш|состав|сгенерир|придума|перевед|отправ|создай|запланируй|оформи|нарисуй/i;

  function assistantAnswerFor(question: string, callId: string | null): AssistantAnswer {
    const base = {
      sources: [],
      fragments: [],
      fragmentTokens: 0,
      windowTokens: 8192 as const,
    };
    if (AS_GEN_RE.test(question)) {
      return {
        ...base,
        kind: 'refusal',
        text: 'Составление текстов — вне области ассистента. Область: поиск и разбор информации в записанных звонках. Могу собрать факты — решения, задачи, сроки.',
      };
    }
    if (/пилот|контракт|план|sow|дедлайн/i.test(question)) {
      const call = sampleCalls[0];
      const title = call?.title ?? 'Звонок';
      return {
        ...base,
        kind: 'answer',
        text: 'Договорились о пилоте на 2 недели, затем полный контракт. Дедлайн контракта — 30 мая; Иван присылает SOW в пятницу.',
        sources: [
          { callId: call?.id ?? 'c1', callTitle: title, startMs: 6000 },
          { callId: call?.id ?? 'c1', callTitle: title, startMs: 12000 },
        ],
        fragments: [
          {
            callId: call?.id ?? 'c1',
            callTitle: title,
            kind: 'transcript',
            speaker: 'Speaker 0',
            startMs: 6000,
            text: 'Сначала пилот на 2 недели, потом полный контракт.',
          },
          {
            callId: call?.id ?? 'c1',
            callTitle: title,
            kind: 'transcript',
            speaker: 'owner',
            startMs: 12000,
            text: 'Согласен. Дедлайн контракта — 30 мая.',
          },
        ],
        fragmentTokens: 1400,
      };
    }
    if (callId) {
      return {
        ...base,
        kind: 'empty',
        text: 'В этом звонке этого не нашлось.',
        escalate: true,
      };
    }
    return {
      ...base,
      kind: 'empty',
      text: 'По звонкам ничего не найдено. Уточните имя участника, тему или период.',
    };
  }

  function assistantMockAsk(ask: { chatId: string | null; callId: string | null; question: string }) {
    const now = new Date().toISOString();
    let entry = ask.chatId
      ? assistantChats.find((c) => c.chat.id === ask.chatId)
      : ask.callId
        ? assistantChats.find((c) => c.chat.callId === ask.callId)
        : undefined;
    if (!entry) {
      const title = ask.question.length > 42 ? `${ask.question.slice(0, 41).trimEnd()}…` : ask.question;
      entry = {
        chat: { id: `mock-chat-${++assistantSeq}`, callId: ask.callId, title, createdAt: now, updatedAt: now },
        messages: [],
        updatedAt: Date.now(),
      };
      assistantChats.push(entry);
    }
    entry.updatedAt = Date.now();
    entry.messages.push({
      id: `mock-msg-${++assistantSeq}`,
      role: 'user',
      text: ask.question,
      answer: null,
      createdAt: now,
    });
    const answer = assistantAnswerFor(ask.question, entry.chat.callId);
    const message: AssistantMessage = {
      id: `mock-msg-${++assistantSeq}`,
      role: 'assistant',
      text: answer.text,
      answer,
      createdAt: now,
    };
    entry.messages.push(message);
    return { chatId: entry.chat.id, message };
  }

  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd: string, args?: unknown) => {
      const a = (args as Record<string, unknown> | undefined) ?? {};
      if (cmd === 'get_setting') {
        const k = a.key as string;
        return settings[k] ?? null;
      }
      if (cmd === 'set_setting') {
        const k = a.key as string;
        const v = a.value as string;
        settings[k] = v;
        return null;
      }
      if (cmd === 'get_call') {
        const id = a.id as string;
        return sampleCalls.find((c) => c.id === id) ?? null;
      }
      if (cmd === 'delete_call') {
        const id = a.id as string;
        const i = sampleCalls.findIndex((c) => c.id === id);
        if (i !== -1) sampleCalls.splice(i, 1);
        return null;
      }
      if (cmd === 'read_call_artifact') {
        const kind = (a.kind ?? a.artifact) as string;
        if (kind === 'recap')
          return '# Рекап\n\n## Главное\n- Договорились о пилоте на 2 недели\n- Дедлайн контракта — 30 мая\n- Иван присылает SOW в пятницу\n\n## Решения\n- Используем managed-тариф\n- Языки: RU + EN';
        if (kind === 'transcript')
          return '## Транскрипт\n\n**Иван:** Давайте обсудим план.\n\n**Дамир:** Я готов. С чего начнём?\n\n**Иван:** Сначала пилот на 2 недели, потом полный контракт.';
        if (kind === 'raw_stt')
          return JSON.stringify({
            version: 1,
            merged: [
              { start: 0, end: 3.5, text: 'Давайте обсудим план.', speakerTag: 'Speaker 0' },
              { start: 3.5, end: 6.0, text: 'Я готов. С чего начнём?', speakerTag: 'owner' },
              { start: 6.0, end: 12.0, text: 'Сначала пилот на 2 недели, потом полный контракт.', speakerTag: 'Speaker 0' },
              { start: 12.0, end: 14.0, text: 'Согласен. Дедлайн контракта — 30 мая.', speakerTag: 'owner' },
            ],
          });
        return null;
      }
      if (cmd === 'list_call_action_items') {
        return [
          { id: 't1', text: 'Прислать SOW до пятницы', owner_contact_id: sampleContact.id, due: '2026-05-23', done: false },
          { id: 't2', text: 'Подписать NDA', owner_contact_id: null, due: null, done: true },
        ];
      }
      // #38 account session
      if (cmd === 'get_account_session_status') {
        return { present: accountSessionToken !== null };
      }
      if (cmd === 'set_account_session') {
        const v = (a.token as string)?.trim() ?? '';
        accountSessionToken = v || null;
        return null;
      }
      if (cmd === 'clear_account_session') {
        accountSessionToken = null;
        return null;
      }
      if (cmd === 'read_account_session_token') {
        return accountSessionToken;
      }
      if (cmd === 'list_byo_status') {
        return ['soniox', 'gladia', 'anthropic'].map((p) => ({
          provider: p,
          present: !!byoKeys[p],
        }));
      }
      if (cmd === 'set_byo_key') {
        const p = a.provider as string;
        const v = (a.value as string)?.trim() ?? '';
        if (!v) {
          delete byoKeys[p];
        } else {
          byoKeys[p] = v;
        }
        return null;
      }
      if (cmd === 'delete_byo_key') {
        const p = a.provider as string;
        delete byoKeys[p];
        return null;
      }
      // #26: in-memory speakers store для preview confirmation flow.
      if (cmd === 'list_call_speakers') {
        const callId = a.callId as string;
        return [
          {
            id: `s-${callId}-1`,
            call_id: callId,
            speaker_tag: 'S1',
            contact_id: speakerBindings[`${callId}:S1`] ?? null,
            contact_display_name:
              speakerBindings[`${callId}:S1`] === ownerContact.id
                ? ownerContact.display_name
                : speakerBindings[`${callId}:S1`] === sampleContact.id
                  ? sampleContact.display_name
                  : null,
            suggestion_contact_id: ownerContact.id,
            suggestion_contact_display_name: ownerContact.display_name,
            suggestion_score: 0.91,
            suggestion_source: 'embedding',
            confirmed: !!speakerBindings[`${callId}:S1`],
          },
          {
            id: `s-${callId}-2`,
            call_id: callId,
            speaker_tag: 'S2',
            contact_id: speakerBindings[`${callId}:S2`] ?? null,
            contact_display_name:
              speakerBindings[`${callId}:S2`] === sampleContact.id
                ? sampleContact.display_name
                : null,
            suggestion_contact_id: sampleContact.id,
            suggestion_contact_display_name: sampleContact.display_name,
            suggestion_score: 0.78,
            suggestion_source: 'both',
            confirmed: !!speakerBindings[`${callId}:S2`],
          },
        ];
      }
      if (cmd === 'confirm_call_speaker') {
        // dev mock: ID формат 's-<callId>-<n>' → restore tag.
        const sid = a.callSpeakerId as string;
        const parts = sid.split('-');
        const tag = `S${parts[parts.length - 1]}`;
        const callId = parts.slice(1, -1).join('-');
        speakerBindings[`${callId}:${tag}`] = a.contactId as string;
        return null;
      }
      if (cmd === 'regenerate_recap') {
        // Dev mock: симулируем задержку, чтобы button spinner был видим.
        await new Promise((r) => setTimeout(r, 800));
        return null;
      }
      if (cmd === 'get_call_audio_path') {
        // В dev-browser нет реальных WAV — возвращаем error чтобы плеер
        // показал «нет аудио» (миро в Tauri будет рабочий файл).
        throw new Error('audio file не найден (dev mock)');
      }
      if (cmd === 'reprocess_call') {
        // [V8] В реальном Tauri это spawn'ит task и сразу возвращается.
        await new Promise((r) => setTimeout(r, 100));
        return null;
      }
      if (cmd === 'cancel_reprocess') {
        await new Promise((r) => setTimeout(r, 50));
        return null;
      }
      if (cmd === 'get_active_pipeline_count') {
        // [V9] In dev browser mock pipeline_tasks always empty (нет реального
        // Rust state); считаем демо processing звонок «активным» для UI showcase.
        return 1;
      }
      // #45: in-memory voice samples preview.
      if (cmd === 'list_voice_samples') {
        const cid = a.contactId as string;
        return voiceSamples.filter((v) => v.contact_id === cid);
      }
      if (cmd === 'delete_voice_sample') {
        const sid = a.id as string;
        const i = voiceSamples.findIndex((v) => v.id === sid);
        if (i !== -1) voiceSamples.splice(i, 1);
        return null;
      }
      if (cmd === 'unbind_call_speaker') {
        const sid = a.callSpeakerId as string;
        const parts = sid.split('-');
        const tag = `S${parts[parts.length - 1]}`;
        const callId = parts.slice(1, -1).join('-');
        delete speakerBindings[`${callId}:${tag}`];
        return null;
      }
      // [W4] Widget window controls. In dev/browser mock there is no second
      // Tauri window — we just acknowledge so the UI doesn't surface noisy
      // errors during component dev/storybook flows.
      if (
        cmd === 'show_recording_widget' ||
        cmd === 'hide_recording_widget' ||
        cmd === 'restore_main_window'
      ) {
        return null;
      }
      // ── [B24.2] Ассистент: in-memory чаты + банк ответов (адаптация
      // wk2-assistant.jsx хендоффа). Задержка ответа 800мс по SPEC §4.
      if (cmd === 'assistant_index_stats') {
        const ready = sampleCalls.filter((c) => c.status === 'ready').length;
        return {
          indexedCalls: ready,
          totalCalls: sampleCalls.length,
          totalDurationSec: sampleCalls
            .filter((c) => c.status === 'ready')
            .reduce((s, c) => s + (c.duration_sec ?? 0), 0),
        };
      }
      if (cmd === 'assistant_list_chats') {
        // Как в проде: свежая активность сверху (ORDER BY updated_at DESC).
        return assistantChats
          .filter((c) => c.chat.callId === null)
          .sort((x, y) => y.updatedAt - x.updatedAt)
          .map((c) => c.chat);
      }
      if (cmd === 'assistant_get_chat') {
        const id = a.chatId as string;
        return assistantChats.find((c) => c.chat.id === id)?.messages ?? [];
      }
      if (cmd === 'assistant_get_call_thread') {
        const cid = a.callId as string;
        return assistantChats.find((c) => c.chat.callId === cid) ?? null;
      }
      if (cmd === 'assistant_delete_chat') {
        const id = a.chatId as string;
        const i = assistantChats.findIndex((c) => c.chat.id === id);
        if (i !== -1) assistantChats.splice(i, 1);
        return null;
      }
      // [B26.4] Полный текст фрагмента: мок хранит полные тексты в answer —
      // отдаём как есть (мок не усекает).
      if (cmd === 'share_text') return null; // [B27.6] нативный пикер в браузере недоступен
      if (cmd === 'assistant_get_fragment_text') {
        const mid = a.messageId as string;
        const idx = a.fragmentIndex as number;
        for (const c of assistantChats) {
          const m = c.messages.find((m) => m.id === mid);
          const t = m?.answer?.fragments[idx]?.text;
          if (t != null) return t;
        }
        throw new Error('fragment not found');
      }
      // [B25] Тумблер семантического поиска: в моке просто in-memory флаг.
      if (cmd === 'assistant_get_semantic_search') {
        return mockSemanticSearch;
      }
      if (cmd === 'assistant_set_semantic_search') {
        mockSemanticSearch = a.enabled as boolean;
        return null;
      }
      if (cmd === 'assistant_ask') {
        const ask = a.args as { chatId: string | null; callId: string | null; question: string };
        await new Promise((r) => setTimeout(r, 800));
        return assistantMockAsk(ask);
      }
      return responses[cmd] ?? null;
    },
  };
}

export {};
