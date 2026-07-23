// [B24.4] Раздел «Ассистент» — точь-в-точь wk2-assistant.jsx:159-250.
// Колонка чатов 232px (группировка по дням, trash по hover) + тред + композер.
// Данные: useAssistantChats (module-кэш = keep-alive между видами).

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { getAssistantIndexStats } from '../api/assistant';
import type { AssistantChatMeta, AssistantIndexStats } from '@wotold/contracts';
import { AskThread } from '../components/assistant/AskThread';
import { AssistantComposer } from '../components/assistant/AssistantComposer';
import { SUGGESTIONS, pickSuggestions } from '../components/assistant/suggestions';
import { useAssistantChats } from '../hooks/useAssistantChats';
import { useI18n, type TranslationKey } from '../i18n';
import { Button, Chip, Icon, IconBtn, Tooltip, useToast } from '../ui';
import { fuzzyFilter } from '../lib/fuzzy';
import { ViewHead } from '../ui/ViewHead';

export interface AssistantPageProps {
  onOpenCall: (callId: string) => void;
  /** [B26.9] Запрос открыть чат (клик в «Недавних»); seq — повторные клики. */
  openChatRequest?: { id: string; seq: number } | null;
  /** [B26.R] Ack: запрос потреблён — родитель сбрасывает его в null, иначе
   *  ремаунт страницы (смена view) заново откроет старый чат. */
  onOpenChatHandled?: () => void;
  /** [B26.5] Чип контакт-источника → раздел «Контакты». */
  onOpenContacts?: () => void;
}

interface ChatGroup {
  label: string;
  items: AssistantChatMeta[];
}

// [B26.11] Панель чатов: границы resize + persist в localStorage
// (паттерн рейла: App.tsx readSavedRailW / wk-railw).
const ASCHATS_MIN = 180;
const ASCHATS_MAX = 400;
const ASCHATS_DEFAULT = 232;
const ASCHATS_COLLAPSE_AT = 150;

/** Чистый clamp ширины панели (unit-тестируется). */
export function clampChatsWidth(w: number): number {
  return Math.max(ASCHATS_MIN, Math.min(ASCHATS_MAX, w));
}

function readSavedChatsW(): number {
  try {
    const v = parseInt(localStorage.getItem('wk-aschatsw') ?? '', 10);
    return v >= ASCHATS_MIN && v <= ASCHATS_MAX ? v : ASCHATS_DEFAULT;
  } catch {
    return ASCHATS_DEFAULT;
  }
}

function readSavedChatsCollapsed(): boolean {
  try {
    return localStorage.getItem('wk-aschats-collapsed') === '1';
  } catch {
    return false;
  }
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

export function AssistantPage({
  onOpenCall,
  openChatRequest,
  onOpenChatHandled,
  onOpenContacts,
}: AssistantPageProps) {
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
  // [B26.11] Панель чатов: ширина/collapse (persist) + fuzzy-поиск.
  const [panelW, setPanelW] = useState<number>(readSavedChatsW);
  const [panelCollapsed, setPanelCollapsed] = useState<boolean>(readSavedChatsCollapsed);
  const [chatQuery, setChatQuery] = useState('');

  useEffect(() => {
    try {
      localStorage.setItem('wk-aschatsw', String(panelW));
      localStorage.setItem('wk-aschats-collapsed', panelCollapsed ? '1' : '0');
    } catch {
      // localStorage недоступен — не критично
    }
  }, [panelW, panelCollapsed]);

  // Drag правой грани (паттерн рейла App.tsx onResizeStart).
  const onPanelResizeStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const sx = e.clientX;
      const sw = panelW;
      const end = () => {
        document.removeEventListener('mousemove', move);
        document.removeEventListener('mouseup', end);
        document.body.style.cursor = '';
        document.body.style.userSelect = '';
      };
      const move = (ev: MouseEvent) => {
        const w = sw + (ev.clientX - sx);
        if (w < ASCHATS_COLLAPSE_AT) {
          setPanelCollapsed(true);
          end();
          return;
        }
        setPanelW(clampChatsWidth(w));
      };
      document.addEventListener('mousemove', move);
      document.addEventListener('mouseup', end);
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
    },
    [panelW],
  );

  useEffect(() => {
    getAssistantIndexStats()
      .then(setStats)
      .catch((e) => console.warn('assistant stats:', e));
  }, []);

  // [B26.9] Открытие чата по клику из «Недавних» (seq — повторные клики).
  // [B26.R] После потребления ack'аем родителю — без сброса запрос пережил бы
  // ремаунт страницы и молча перебил бы выбранный руками чат.
  useEffect(() => {
    if (openChatRequest) {
      void openChat(openChatRequest.id);
      onOpenChatHandled?.();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openChatRequest?.seq]);

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

  // [B26.11] Активный поиск: fuzzy-фильтр по титулам, плоский список по
  // score (day-группировка не имеет смысла при ранжировании).
  const searching = chatQuery.trim().length > 0;
  const filteredChats = searching ? fuzzyFilter(chats, chatQuery, (c) => c.title) : chats;
  const groups: ChatGroup[] = [];
  if (searching) {
    if (filteredChats.length > 0) {
      groups.push({ label: t('assistant.searchResults'), items: filteredChats });
    }
  } else {
    for (const chat of filteredChats) {
      const label = dayLabel(chat.createdAt);
      const last = groups[groups.length - 1];
      if (last && last.label === label) {
        last.items.push(chat);
      } else {
        groups.push({ label, items: [chat] });
      }
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

  // [B27.3] Название активного чата — в хедер вместо чипа статистики.
  const activeChatTitle = activeChatId
    ? chats.find((c) => c.id === activeChatId)?.title ?? null
    : null;

  // [B27.4] Случайные подсказки на маунт; смена языка перевыбирает.
  const suggests = useMemo(() => pickSuggestions(SUGGESTIONS[locale]), [locale]);

  return (
    // [B24.7] Shared shell (канон Inbox/Contacts, B18.9-fix): bleed мимо
    // паддинга .app-main 34/44 + fill вьюпорта — .view-head флашится к краям,
    // .as-layout получает всю высоту, composer-dock прижат к низу.
    <div className="main" style={{ margin: '-34px -44px', height: '100vh' }}>
      <ViewHead icon="chat" title={t('assistant.title')}>
        {/* [B27.3] Активный чат → его полное название; иначе — чип статистики. */}
        {activeChatTitle ? (
          <span className="as-head-chat u-trunc" title={activeChatTitle}>
            {activeChatTitle}
          </span>
        ) : statsChip ? (
          <Tooltip content={t('assistant.statsTooltip')} side="bottom">
            <Chip size="sm" tone="line" icon="doc">
              {statsChip}
            </Chip>
          </Tooltip>
        ) : null}
      </ViewHead>
      <div className="as-layout">
        <div
          className="as-chats"
          data-collapsed={panelCollapsed || undefined}
          style={{ ['--aschats-w' as string]: `${panelW}px` } as React.CSSProperties}
        >
          {panelCollapsed ? (
            // [B26.11] Свёрнутая панель: «Новый чат» иконкой + развернуть.
            <div className="as-chats-mini">
              <IconBtn
                icon="plus"
                label={t('assistant.newChat')}
                tip={t('assistant.newChat')}
                onClick={() => {
                  setPanelCollapsed(false);
                  startNewChat();
                }}
              />
              <IconBtn
                icon="chevronRight"
                label={t('assistant.expandPanel')}
                tip={t('assistant.expandPanel')}
                onClick={() => setPanelCollapsed(false)}
              />
            </div>
          ) : (
            <>
          {/* [B27.2] Цельный header панели: кнопка+collapse и поиск с иконкой. */}
          <div className="as-chats-head">
            <div className="as-chats-head-row">
              <Button
                variant="default"
                size="sm"
                block
                leading={<Icon name="plus" size={14} />}
                onClick={startNewChat}
              >
                {t('assistant.newChat')}
              </Button>
              <IconBtn
                icon="chevronLeft"
                label={t('assistant.collapsePanel')}
                tip={t('assistant.collapsePanel')}
                onClick={() => setPanelCollapsed(true)}
              />
            </div>
            {/* [B26.11] Fuzzy-поиск по титулам чатов; Esc — сброс. */}
            <label className="input as-search">
              <Icon name="search" size={14} className="iico" />
              <input
                placeholder={t('assistant.searchChats')}
                aria-label={t('assistant.searchChats')}
                value={chatQuery}
                onChange={(e) => setChatQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Escape') setChatQuery('');
                }}
              />
            </label>
          </div>
          <div className="as-chats-list scroll">
            {groups.map((g) => (
              <div key={g.label}>
                <div className="sec-label">
                  <span>{g.label}</span>
                </div>
                {/* [B24.7 a11y C1/M3] Строка = контейнер + настоящая кнопка
                    открытия + соседняя (НЕ вложенная) кнопка удаления. */}
                <ul className="as-chat-ul">
                  {g.items.map((chat) => (
                    <li
                      key={chat.id}
                      className="navitem"
                      data-active={activeChatId === chat.id ? 'true' : undefined}
                    >
                      <button
                        type="button"
                        className="as-chat-open"
                        onClick={() => void openChat(chat.id)}
                        aria-current={activeChatId === chat.id ? 'true' : undefined}
                      >
                        <span className="nav-ico">
                          <Icon name="chat" size={15} />
                        </span>
                        <span className="nav-label u-trunc">{chat.title}</span>
                      </button>
                      <span className="as-del">
                        <IconBtn
                          icon="trash"
                          size="sm"
                          label={t('assistant.deleteChat')}
                          onClick={() => void deleteChat(chat.id)}
                        />
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
            {!searching && chats.length === 0 && (
              <div className="u-muted" style={{ padding: '10px 12px', fontSize: 12.5 }}>
                {t('assistant.noChats')}
              </div>
            )}
            {searching && filteredChats.length === 0 && (
              <div className="u-muted" style={{ padding: '10px 12px', fontSize: 12.5 }}>
                {t('assistant.searchEmpty')}
              </div>
            )}
          </div>
            </>
          )}
          {!panelCollapsed && (
            // eslint-disable-next-line jsx-a11y/no-static-element-interactions
            <div className="as-resize" onMouseDown={onPanelResizeStart} aria-hidden="true" />
          )}
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
                  {suggests.map((s) => (
                    <Chip key={s} tone="line" icon="arrowRight" onClick={() => void ask(s)}>
                      {s}
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
                  onOpenContacts={onOpenContacts}
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
    </div>
  );
}
