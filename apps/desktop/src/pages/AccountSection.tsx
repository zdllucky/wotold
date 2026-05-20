import { useEffect, useState } from 'react';
import { humanError } from '../api/errors';
import { open as openExternal } from '@tauri-apps/plugin-shell';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import {
  clearAccountSession,
  fetchMe,
  setAccountSession,
  signOut as apiSignOut,
  startSignIn,
  type AccountIdentity,
  type OidcProvider,
} from '../api/auth';
import { Badge, Button, Card, InputField } from '../ui';

interface AuthDeepLinkPayload {
  path: string;
  session?: string;
  provider?: string;
  email?: string;
  name?: string;
}

interface ProviderMeta {
  id: OidcProvider;
  label: string;
  // Apple/Microsoft пока stub'ы на прокси (#37 X4 deferred).
  disabled?: boolean;
}

const PROVIDERS: ProviderMeta[] = [
  { id: 'google', label: 'Google' },
  { id: 'apple', label: 'Apple', disabled: true },
  { id: 'microsoft', label: 'Microsoft', disabled: true },
];

type State =
  | { kind: 'loading' }
  | { kind: 'signed_out' }
  | { kind: 'pending_paste'; provider: OidcProvider; authorizeUrl: string }
  | { kind: 'signed_in'; identity: AccountIdentity; expiresAt: string };

export function AccountSection() {
  const [state, setState] = useState<State>({ kind: 'loading' });
  const [pasteValue, setPasteValue] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const me = await fetchMe();
      if (me) {
        setState({
          kind: 'signed_in',
          identity: me.account,
          expiresAt: me.session.expiresAt,
        });
      } else {
        setState({ kind: 'signed_out' });
      }
      setError(null);
    } catch (e) {
      setError(humanError(e));
      setState({ kind: 'signed_out' });
    }
  };

  useEffect(() => {
    void refresh();

    // [B9]: Tauri deep-link → wotold://auth/callback?session=... — авто-перехват session.
    let unlisten: UnlistenFn | undefined;
    listen<AuthDeepLinkPayload>('auth:deep-link', async (ev) => {
      const session = ev.payload?.session?.trim();
      if (!session) return;
      try {
        await setAccountSession(session);
        await refresh();
      } catch (e) {
        setError(humanError(e));
      }
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((e: unknown) => {
        console.warn('auth:deep-link listener:', e);
      });
    return () => {
      unlisten?.();
    };
  }, []);

  const onStart = async (provider: OidcProvider) => {
    setBusy(true);
    setError(null);
    try {
      // [B9]: prod использует deep-link wotold:// для авто-перехвата.
      // Dev mode: LaunchServices не успевает зарегистрировать scheme для
      // каждой пересборки → Safari показывает «адрес недействителен».
      // Fallback на 'json' — пользователь видит sessionId в браузере и
      // копирует в paste-форму ниже.
      const redirectMode: 'json' | 'deeplink' = import.meta.env.DEV ? 'json' : 'deeplink';
      const { authorizeUrl } = await startSignIn(provider, undefined, redirectMode);
      await openExternal(authorizeUrl);
      setState({ kind: 'pending_paste', provider, authorizeUrl });
    } catch (e) {
      setError(humanError(e));
    } finally {
      setBusy(false);
    }
  };

  const onCompletePaste = async () => {
    const trimmed = pasteValue.trim();
    if (!trimmed) {
      setError('Введи session token из браузера.');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await setAccountSession(trimmed);
      setPasteValue('');
      await refresh();
    } catch (e) {
      setError(humanError(e));
    } finally {
      setBusy(false);
    }
  };

  const onCancelPaste = () => {
    setPasteValue('');
    setState({ kind: 'signed_out' });
    setError(null);
  };

  const onSignOut = async () => {
    setBusy(true);
    setError(null);
    try {
      await apiSignOut();
      await refresh();
    } catch (e) {
      setError(humanError(e));
      // Локальное удаление как fallback.
      try {
        await clearAccountSession();
      } catch {
        // ignore
      }
      setState({ kind: 'signed_out' });
    } finally {
      setBusy(false);
    }
  };

  if (state.kind === 'loading') {
    return <p className="muted">Загрузка…</p>;
  }

  return (
    <div>
      <p
        className="muted"
        style={{
          fontFamily: 'var(--font-serif)',
          fontStyle: 'italic',
          fontSize: 14,
          marginTop: 0,
          marginBottom: 14,
        }}
      >
        Облачная синхронизация скоро. Сейчас вход в аккаунт ничего не
        разблокирует — Wotold полностью работает локально без логина.
      </p>
      {error && (
        <p
          style={{
            color: 'var(--signal)',
            fontFamily: 'var(--font-sans)',
            marginBottom: 12,
          }}
        >
          {error}
        </p>
      )}

      {state.kind === 'signed_in' && (
        <SignedInView
          identity={state.identity}
          expiresAt={state.expiresAt}
          busy={busy}
          onSignOut={onSignOut}
        />
      )}

      {state.kind === 'pending_paste' && (
        <PendingPasteView
          provider={state.provider}
          authorizeUrl={state.authorizeUrl}
          pasteValue={pasteValue}
          onChange={setPasteValue}
          onComplete={onCompletePaste}
          onCancel={onCancelPaste}
          busy={busy}
        />
      )}

      {state.kind === 'signed_out' && (
        <SignedOutView busy={busy} onStart={onStart} />
      )}
    </div>
  );
}

function SignedInView({
  identity,
  expiresAt,
  busy,
  onSignOut,
}: {
  identity: AccountIdentity;
  expiresAt: string;
  busy: boolean;
  onSignOut: () => void;
}) {
  return (
    <Card variant="sunken">
      <div
        style={{
          display: 'flex',
          alignItems: 'flex-start',
          justifyContent: 'space-between',
          gap: 12,
          marginBottom: 10,
        }}
      >
        <div style={{ minWidth: 0 }}>
          <p
            style={{
              fontFamily: 'var(--font-serif)',
              fontSize: 18,
              color: 'var(--ink)',
              margin: 0,
            }}
          >
            {identity.displayName ?? identity.email ?? identity.id}
          </p>
          {identity.email && identity.displayName && (
            <p
              className="muted"
              style={{ margin: '2px 0 0', fontSize: 13 }}
            >
              {identity.email}
            </p>
          )}
        </div>
        <Badge tone="success">{identity.provider}</Badge>
      </div>
      <p className="small-caps" style={{ marginBottom: 14 }}>
        Session действует до {formatDate(expiresAt)}
      </p>
      <div style={{ display: 'flex', gap: 8 }}>
        <Button variant="danger" size="sm" onClick={onSignOut} disabled={busy} busy={busy}>
          Выйти
        </Button>
      </div>
    </Card>
  );
}

function PendingPasteView({
  provider,
  authorizeUrl,
  pasteValue,
  onChange,
  onComplete,
  onCancel,
  busy,
}: {
  provider: OidcProvider;
  authorizeUrl: string;
  pasteValue: string;
  onChange: (v: string) => void;
  onComplete: () => void;
  onCancel: () => void;
  busy: boolean;
}) {
  return (
    <Card variant="sunken">
      <p
        style={{
          fontFamily: 'var(--font-serif)',
          fontSize: 15,
          margin: '0 0 8px',
        }}
      >
        <strong style={{ fontWeight: 500 }}>Шаг 1.</strong> В браузере открылась
        страница входа <Badge tone="accent">{provider}</Badge>. Войди и подтверди.
      </p>
      <p
        style={{
          fontFamily: 'var(--font-serif)',
          fontSize: 15,
          margin: '0 0 12px',
        }}
      >
        <strong style={{ fontWeight: 500 }}>Шаг 2.</strong> После успешного входа
        прокси покажет JSON с полем <code className="mono">sessionId</code>.
        Скопируй значение sessionId и вставь сюда.
      </p>
      <InputField
        label="Session ID"
        type="password"
        placeholder="UUID из ответа прокси"
        value={pasteValue}
        onChange={(e) => onChange(e.target.value)}
        disabled={busy}
      />
      <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
        <Button variant="ghost" size="sm" onClick={onCancel} disabled={busy}>
          Отмена
        </Button>
        <Button
          variant="primary"
          size="sm"
          onClick={onComplete}
          disabled={busy || !pasteValue.trim()}
          busy={busy}
        >
          Подтвердить
        </Button>
      </div>
      <p
        className="muted"
        style={{
          fontStyle: 'italic',
          fontFamily: 'var(--font-serif)',
          fontSize: 13,
          marginTop: 14,
          marginBottom: 4,
        }}
      >
        Авто-перехват callback (без копи-пасты) — в плане через deep-link
        (<code className="mono">wotold://</code>).
      </p>
      <p
        className="subtle mono"
        style={{ fontSize: 11, margin: 0, wordBreak: 'break-all' }}
      >
        {authorizeUrl}
      </p>
    </Card>
  );
}

function SignedOutView({
  busy,
  onStart,
}: {
  busy: boolean;
  onStart: (p: OidcProvider) => void;
}) {
  return (
    <Card variant="sunken">
      <p
        style={{
          fontFamily: 'var(--font-serif)',
          fontSize: 15,
          margin: '0 0 12px',
        }}
      >
        Войти через SSO. Откроется браузер.
      </p>
      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
        {PROVIDERS.map((p) => (
          <Button
            key={p.id}
            variant={p.disabled ? 'ghost' : 'secondary'}
            size="sm"
            onClick={() => onStart(p.id)}
            disabled={p.disabled || busy}
          >
            {p.label}
            {p.disabled && (
              <Badge tone="neutral" style={{ marginLeft: '0.4rem' }}>
                скоро
              </Badge>
            )}
          </Button>
        ))}
      </div>
    </Card>
  );
}

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleDateString('ru-RU', {
      day: '2-digit',
      month: '2-digit',
      year: 'numeric',
    });
  } catch {
    return iso;
  }
}
