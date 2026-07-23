// [B26.6] Строка фрагмента «Контекста поиска». Текст приходит с бэка
// УСЕЧЁННЫМ (B26.4) — chevron-раскрытие лениво грузит полный текст командой
// и при сворачивании ВЫЧИЩАЕТ его из state (ушёл из DOM и JS-памяти).

import { useState } from 'react';
import type { AssistantFragment } from '@wotold/contracts';

import { getAssistantFragmentText } from '../../api/assistant';
import { useI18n } from '../../i18n';
import { Icon } from '../../ui';
import { fmtSourceClock, speakerColor } from './AnswerMsg';

export interface FragmentRowProps {
  fragment: AssistantFragment;
  /** Индекс фрагмента в answer.fragments (ключ ленивой подгрузки). */
  index: number;
  /** id assistant-сообщения (answer_json — источник полного текста). */
  messageId: string;
}

export function FragmentRow({ fragment: f, index, messageId }: FragmentRowProps) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [fullText, setFullText] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);

  const toggle = async () => {
    if (open) {
      // Сворачивание: полный текст вычищается из state → DOM/память чистые.
      setOpen(false);
      setFullText(null);
      return;
    }
    setError(false);
    setOpen(true);
    if (fullText == null) {
      setLoading(true);
      try {
        setFullText(await getAssistantFragmentText(messageId, index));
      } catch {
        setError(true);
        setOpen(false);
      } finally {
        setLoading(false);
      }
    }
  };

  const body = open ? (loading ? null : (fullText ?? f.text)) : f.text;

  return (
    <div className="frag">
      <b style={{ color: speakerColor(f.callId, f.speaker) }}>{f.speaker ?? f.callTitle}</b>
      <span className="u-muted">
        {' '}
        · {f.callTitle}
        {f.startMs != null ? ` · ${fmtSourceClock(f.startMs)}` : ''}
      </span>
      <br />
      {body}
      {loading && <span className="u-muted">{t('assistant.fragLoading')}</span>}
      {f.textTruncated && (
        <button
          type="button"
          className="frag-more"
          aria-expanded={open}
          onClick={() => void toggle()}
        >
          <Icon
            name="chevronRight"
            size={11}
            style={{ transform: open ? 'rotate(90deg)' : undefined }}
          />
          {open ? t('assistant.fragCollapse') : t('assistant.fragExpand')}
        </button>
      )}
      {error && <span className="frag-error">{t('assistant.fragLoadError')}</span>}
    </div>
  );
}
