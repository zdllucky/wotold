// Dev-only mock for Tauri invoke API when running plain Vite in browser (no webview).
// Lets us preview pages visually without spinning up `tauri dev`. Inert in production.

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
      if (cmd === 'unbind_call_speaker') {
        const sid = a.callSpeakerId as string;
        const parts = sid.split('-');
        const tag = `S${parts[parts.length - 1]}`;
        const callId = parts.slice(1, -1).join('-');
        delete speakerBindings[`${callId}:${tag}`];
        return null;
      }
      return responses[cmd] ?? null;
    },
  };
}

export {};
