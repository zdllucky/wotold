// [B24.4] Раздел «Ассистент» — точь-в-точь wk2-assistant.jsx:159-250.
// Колонка чатов 232px (группировка по дням, trash по hover) + тред + композер.
// Данные: useAssistantChats (module-кэш = keep-alive между видами).

import { useEffect, useRef, useState } from 'react';

import { getAssistantIndexStats } from '../api/assistant';
import type { AssistantChatMeta, AssistantIndexStats } from '@wotold/contracts';
import { AskThread } from '../components/assistant/AskThread';
import { AssistantComposer } from '../components/assistant/AssistantComposer';
import { useAssistantChats } from '../hooks/useAssistantChats';
import { useI18n, type TranslationKey } from '../i18n';
import { Button, Chip, Icon, IconBtn, useToast } from '../ui';

export interface AssistantPageProps {
  onOpenCall: (callId: string) => void;
}

const SUGGEST_KEYS: TranslationKey[] = [
  'assistant.suggest1',
  'assistant.suggest2',
  'assistant.suggest3',
  'assistant.suggest4',
];

interface ChatGroup {
  label: string;
  items: AssistantChatMeta[];
}

/** 3-формный плюрал (ru-правило; en/kk-словари несут свои формы под теми же
 * ключами): 1/21/31 → One, 2–4/22–24 → Few, остальное → Many. */
function ruPluralKey(n: number): TranslationKey {
  const mod10 = n % 10;
  const mod100 = n % 100;
  if (mod10 === 1 && mod100 !== 11) return 'assistant.callsPluralOne';
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return 'assistant.callsPluralFew';
  return 'assistant.callsPluralMany';
}

export function AssistantPage({ onOpenCall }: AssistantPageProps) {
  const { t, locale } = useI18n();
  const toast = useToast();
  const {
    chats,
    activeChatId,
    messages,
    pending,
    error,
    ask,
    openChat,
    startNewChat,
    deleteChat,
  } = useAssistantChats();
  const [stats, setStats] = useState<AssistantIndexStats | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    getAssistantIndexStats()
      .then(setStats)
      .catch((e) => console.warn('assistant stats:', e));
  }, []);

  // Ошибки ask/open/delete — тостом (SPEC: честные состояния, консоль чистая).
  useEffect(() => {
    if (error) toast.show({ message: error, tone: 'danger' });
  }, [error, toast]);

  // Автоскролл к концу треда (мок :167). Зависимость — идентичность массива:
  // смена чата с равной длиной тоже должна скроллить (ревью).
  useEffect(() => {
    requestAnimationFrame(() => {
      const el = scrollRef.current;
      if (el) el.scrollTop = el.scrollHeight;
    });
  }, [messages, pending]);

  const dayLabel = (iso: string): string => {
    const d = new Date(iso);
    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const that = new Date(d.getFullYear(), d.getMonth(), d.getDate());
    const diffDays = Math.round((today.getTime() - that.getTime()) / 86_400_000);
    if (diffDays <= 0) return t('assistant.dayToday');
    if (diffDays === 1) return t('assistant.dayYesterday');
    return d.toLocaleDateString(locale, { day: 'numeric', month: 'long' });
  };

  const groups: ChatGroup[] = [];
  for (const chat of chats) {
    const label = dayLabel(chat.createdAt);
    const last = groups[groups.length - 1];
    if (last && last.label === label) {
      last.items.push(chat);
    } else {
      groups.push({ label, items: [chat] });
    }
  }

  const statsChip = (() => {
    if (!stats) return null;
    const totalMin = Math.round(stats.totalDurationSec / 60);
    const dur =
      totalMin >= 60
        ? t('assistant.durHours', { h: Math.floor(totalMin / 60), m: totalMin % 60 })
        : t('assistant.durMinutes', { m: totalMin });
    const plural = t(ruPluralKey(stats.totalCalls));
    return t('assistant.statsChip', {
      ready: stats.indexedCalls,
      total: stats.totalCalls,
      plural,
      dur,
    });
  })();

  const pendingText = t('assistant.pendingGlobal', { n: stats?.indexedCalls ?? 0 });

  return (
    <>
      <div className="view-head">
        <Icon name="chat" size={17} style={{ color: 'var(--text-3)' }} />
        <span style={{ fontWeight: 650, fontSize: 'var(--t-14)' }}>{t('assistant.title')}</span>
        {statsChip && (
          <span className="tip tip--bottom" data-tip={t('assistant.statsTooltip')}>
            <Chip size="sm" tone="line" icon="doc">
              {statsChip}
            </Chip>
          </span>
        )}
      </div>
      <div className="as-layout">
        <div className="as-chats">
          <div style={{ padding: '10px 10px 4px' }}>
            <Button variant="default" size="sm" block leading={<Icon name="plus" size={14} />} onClick={startNewChat}>
              {t('assistant.newChat')}
            </Button>
          </div>
          <div className="as-chats-list scroll">
            {groups.map((g) => (
              <div key={g.label}>
                <div className="sec-label">
                  <span>{g.label}</span>
                </div>
                {g.items.map((chat) => (
                  <div
                    key={chat.id}
                    role="button"
                    tabIndex={0}
                    className="navitem"
                    data-active={activeChatId === chat.id ? 'true' : undefined}
                    onClick={() => void openChat(chat.id)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        void openChat(chat.id);
                      }
                    }}
                  >
                    <span className="nav-ico">
                      <Icon name="chat" size={15} />
                    </span>
                    <span className="nav-label u-trunc">{chat.title}</span>
                    <span
                      className="as-del"
                      onClick={(e) => {
                        e.stopPropagation();
                        void deleteChat(chat.id);
                      }}
                    >
                      <IconBtn icon="trash" size="sm" label={t('assistant.deleteChat')} />
                    </span>
                  </div>
                ))}
              </div>
            ))}
            {chats.length === 0 && (
              <div className="u-faint" style={{ padding: '10px 12px', fontSize: 12.5 }}>
                {t('assistant.noChats')}
              </div>
            )}
          </div>
        </div>

        <div className="as-main">
          <div className="as-scroll scroll" ref={scrollRef}>
            {!activeChatId && messages.length === 0 && !pending ? (
              <div className="as-empty">
                <div className="as-empty-ico">
                  <Icon name="chat" size={22} />
                </div>
                <div style={{ fontWeight: 650, fontSize: 16 }}>{t('assistant.emptyTitle')}</div>
                <p className="u-muted" style={{ fontSize: 13.5, margin: 0, lineHeight: 1.55 }}>
                  {t('assistant.emptyDesc')}
                </p>
                <div className="ask-suggest" style={{ justifyContent: 'center', marginTop: 10 }}>
                  {SUGGEST_KEYS.map((key) => (
                    <Chip key={key} tone="line" icon="arrowRight" onClick={() => void ask(t(key))}>
                      {t(key)}
                    </Chip>
                  ))}
                </div>
              </div>
            ) : (
              <div className="as-doc">
                <AskThread
                  messages={messages}
                  pending={pending}
                  pendingText={pendingText}
                  onOpenCall={onOpenCall}
                />
              </div>
            )}
          </div>
          <div className="composer-dock">
            <AssistantComposer
              placeholder={t('assistant.composerGlobal')}
              icon="search"
              disabled={pending}
              onAsk={(q) => void ask(q)}
            />
          </div>
        </div>
      </div>
    </>
  );
}
