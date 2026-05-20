import { useEffect, useState } from 'react';
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
      setError(String(e));
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
        setError(String(e));
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
      setError(String(e));
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
      setError(String(e));
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
      setError(String(e));
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
    return <p className="hint">Загрузка…</p>;
  }

  return (
    <div className="account-section">
      <p className="account-section-hint">
        M10.3: аккаунт в MVP ничего не разблокирует — это задел под облачную
        синхронизацию (DEFERRED). Локальный режим работает без логина.
      </p>
      {error && <p className="error">{error}</p>}

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
    <Card variant="sunken" compact>
      <div className="account-row-head">
        <div>
          <p className="account-name">{identity.displayName ?? identity.email ?? identity.id}</p>
          {identity.email && identity.displayName && (
            <p className="account-email">{identity.email}</p>
          )}
        </div>
        <Badge tone="success">{identity.provider}</Badge>
      </div>
      <p className="account-expires">Session действует до {formatDate(expiresAt)}</p>
      <div className="account-actions">
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
    <Card variant="sunken" compact>
      <p className="account-step">
        <strong>Шаг 1.</strong> В браузере открылась страница входа{' '}
        <Badge tone="accent">{provider}</Badge>. Войди и подтверди.
      </p>
      <p className="account-step">
        <strong>Шаг 2.</strong> После успешного входа прокси покажет JSON с
        полем <code>sessionId</code>. Скопируй значение sessionId и вставь сюда.
      </p>
      <InputField
        label="Session ID"
        type="password"
        placeholder="UUID из ответа прокси"
        value={pasteValue}
        onChange={(e) => onChange(e.target.value)}
        disabled={busy}
      />
      <div className="account-actions">
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
      <p className="account-deeplink-hint">
        Авто-перехват callback (без копи-пасты) — в плане через deep-link (`wotold://`).
      </p>
      <p className="text-subtle text-mono">{authorizeUrl}</p>
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
    <Card variant="sunken" compact>
      <p className="account-step">Войти через SSO. Откроется браузер.</p>
      <div className="account-providers">
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
                X4 deferred
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
