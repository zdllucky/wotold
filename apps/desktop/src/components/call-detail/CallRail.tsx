// [B18.3a] Call-detail right rail (Wotold v2 .rrail). Properties + Participants
// + Actions. Participants assign reuses the existing SpeakerConfirmModal flow
// (sample + consent R2): «Определить» calls onIdentify(tag) which the page maps
// to setConfirmingTag. No new APIs; speaker data comes from the page.

import { bcp47, useI18n } from '../../i18n';
import type { Call } from '../../api/recording';
import type { CallSpeakerView } from '../../api/speakers';
import type { SpeakerSample } from '../SpeakerCard';
import { Icon } from '../../ui/Icon';
import { splitParticipants } from './participantGroups';
import { ParticipantRow } from './ParticipantRow';

const SP = ['var(--sp1)', 'var(--sp2)', 'var(--sp3)', 'var(--sp4)', 'var(--sp5)'];

function fmtDate(iso: string, locale: string): string {
  try {
    return new Date(iso).toLocaleDateString(bcp47(locale as Parameters<typeof bcp47>[0]), {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
    });
  } catch {
    return iso;
  }
}

function fmtTime(iso: string, locale: string): string {
  try {
    return new Date(iso).toLocaleTimeString(bcp47(locale as Parameters<typeof bcp47>[0]), {
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return '';
  }
}

function fmtDur(sec: number | null): string {
  if (sec == null) return '—';
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  if (h > 0) return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  return `${m}:${s.toString().padStart(2, '0')}`;
}

interface CallRailProps {
  call: Call;
  speakers: CallSpeakerView[];
  /** Open the speaker-confirm modal for an unconfirmed speaker tag. */
  onIdentify: (tag: string) => void;
  /** [B20.7] Сэмплы голосов (по speaker_tag) для прослушивания в dropdown. */
  samplesByTag: Map<string, SpeakerSample | null>;
  /** [B20.7] Отвязать конкретный голос (call_speaker id) от контакта. */
  onUnbind: (callSpeakerId: string) => void;
  onExport: () => void;
  exporting: boolean;
}

export function CallRail({
  call,
  speakers,
  onIdentify,
  samplesByTag,
  onUnbind,
  onExport,
  exporting,
}: CallRailProps) {
  const { t, locale } = useI18n();
  // [B20.6] Несколько голосов одного контакта → одна строка участника.
  const { confirmed, unconfirmed } = splitParticipants(speakers);
  const undef = unconfirmed.length;
  const peopleCount = confirmed.length + unconfirmed.length;

  const statusChip =
    call.status === 'ready' ? (
      <span className="chip chip--ok">
        <Icon name="check" size={11} />
        {t('inbox.statusReady')}
      </span>
    ) : call.status === 'failed' ? (
      <span className="chip chip--danger">
        <Icon name="alert" size={11} />
        {t('inbox.statusError')}
      </span>
    ) : (
      <span className="chip chip--accent">
        <Icon name="refresh" size={11} />
        {t('inbox.statusProcessing')}
      </span>
    );

  return (
    <aside className="rrail">
      <div className="rrail-scroll scroll">
        <div className="rrail-sec" style={{ marginTop: 0 }}>
          {t('callDetail.railProperties')}
        </div>
        <div className="prop">
          <span className="prop-k">{t('callDetail.railStatus')}</span>
          <span>{statusChip}</span>
        </div>
        {/* [B20.10] Строка «Движок» убрана — engine-инфо только в Настройках. */}
        <div className="prop">
          <span className="prop-k">
            <Icon name="calendar" size={13} />
            {t('callDetail.railDate')}
          </span>
          <span>
            {fmtDate(call.started_at, locale)} · {fmtTime(call.started_at, locale)}
          </span>
        </div>
        <div className="prop">
          <span className="prop-k">
            <Icon name="clock" size={13} />
            {t('callDetail.railDuration')}
          </span>
          <span className="mono">{fmtDur(call.duration_sec)}</span>
        </div>

        <div
          className="rrail-sec"
          style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}
        >
          <span>
            {t('callDetail.railParticipants')} · {peopleCount}
          </span>
          {undef > 0 && (
            <span className="chip chip--warn" data-size="sm">
              {t('callDetail.railUndefined', { n: undef })}
            </span>
          )}
        </div>
        {speakers.length === 0 && (
          <div className="u-faint" style={{ fontSize: 12.5, padding: '4px 0' }}>
            {t('callDetail.railNoSpeakers')}
          </div>
        )}
        {confirmed.map((g, i) => (
          <ParticipantRow
            key={g.key}
            group={g}
            color={SP[i % SP.length]!}
            samplesByTag={samplesByTag}
            onUnbind={onUnbind}
          />
        ))}
        {unconfirmed.map((s) => (
          <div className="lrow" key={s.speaker_tag} style={{ padding: '5px 0', gap: 10 }}>
            <span
              className="avatar"
              style={{
                width: 28,
                height: 28,
                background: 'var(--text-faint)',
                fontSize: 11,
                flex: '0 0 auto',
              }}
            >
              ?
            </span>
            <div style={{ minWidth: 0, flex: 1 }}>
              <div className="u-trunc" style={{ fontWeight: 550, color: 'var(--text-2)' }}>
                {t('callDetail.railSpeakerUnknown')}
              </div>
            </div>
            <button
              type="button"
              className="btn btn--soft"
              data-size="sm"
              onClick={() => onIdentify(s.speaker_tag)}
            >
              {t('callDetail.railIdentify')}
            </button>
          </div>
        ))}

        <div className="rrail-sec">{t('callDetail.railActions')}</div>
        <div style={{ display: 'grid', gap: 6 }}>
          <button
            type="button"
            className="btn btn--default"
            data-block="true"
            disabled={call.status !== 'ready' || exporting}
            onClick={onExport}
          >
            <Icon name="download" size={14} />
            {exporting ? t('callDetail.exportBusy') : t('callDetail.railExport')}
          </button>
        </div>
      </div>
    </aside>
  );
}
