// [V8.1] Loading skeleton для всей CallDetailPage — mimics финальную
// структуру (back-link, meta caps, title, ParticipantsRow, tabs, transcript
// rows). Юзер видит form factor контента до того как загрузятся артефакты.
//
// Параллельно грузятся 9 ресурсов (call meta + recap + transcript + raw_stt
// + tasks + contacts + speakers + 2 audio paths) — на холодном диске может
// занять до пары секунд. Раньше было «Загрузка…», теперь — нормальный shimmer.

import { Skeleton } from '../../ui';
import { useI18n } from '../../i18n';

interface CallDetailSkeletonProps {
  onBack: () => void;
}

export function CallDetailSkeleton({ onBack }: CallDetailSkeletonProps) {
  const { t } = useI18n();
  return (
    <section
      style={{
        display: 'flex',
        flexDirection: 'column',
        minHeight: '100%',
      }}
      aria-busy="true"
    >
      <button
        type="button"
        className="btn btn--quiet"
        onClick={onBack}
        style={{ marginBottom: 18, paddingLeft: 0 }}
      >
        {t('common.backAll')}
      </button>
      <header style={{ marginBottom: 22 }}>
        <Skeleton width="22ch" height="0.75em" style={{ marginBottom: 10 }} />
        <Skeleton width="16ch" height="2.25rem" style={{ marginBottom: 14 }} />
        <div style={{ display: 'flex', gap: 10 }}>
          <Skeleton width="7rem" height="1.5rem" radius="999px" />
          <Skeleton width="6rem" height="1.5rem" radius="999px" />
          <Skeleton width="8rem" height="1.5rem" radius="999px" />
        </div>
      </header>
      {/* Tabs row */}
      <div style={{ display: 'flex', gap: 18, marginBottom: 22 }}>
        <Skeleton width="6rem" height="1rem" />
        <Skeleton width="7rem" height="1rem" />
        <Skeleton width="5rem" height="1rem" />
        <Skeleton width="6rem" height="1rem" />
      </div>
      {/* Transcript ghost rows */}
      <div className="transcript">
        {[0, 1, 2, 3, 4].map((i) => (
          <div key={i} className="transcript-row transcript-row--ghost">
            <div className="transcript-speaker" aria-hidden="true">···</div>
            <div className="transcript-text" aria-hidden="true">···</div>
            <div className="transcript-time" aria-hidden="true">···</div>
          </div>
        ))}
      </div>
    </section>
  );
}
