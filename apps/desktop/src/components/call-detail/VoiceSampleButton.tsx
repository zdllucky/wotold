// [B20.7] Лайт-кнопка прослушивания сэмпла голоса (для строк в dropdown rail):
// hidden <audio>, seek на start сэмпла, автостоп на end. Без waveform/peaks —
// полный плеер остаётся в SpeakerCard (identify-flow).

import { useEffect, useRef, useState } from 'react';
import type { SpeakerSample } from '../SpeakerCard';
import { IconBtn } from '../../ui/IconBtn';
import { useI18n } from '../../i18n';

interface VoiceSampleButtonProps {
  sample: SpeakerSample | null;
}

export function VoiceSampleButton({ sample }: VoiceSampleButtonProps) {
  const { t } = useI18n();
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState(false);

  // Смена сэмпла/анмаунт → стоп. Элемент захватываем на setup: в cleanup
  // React уже обнулил ref (иначе detached <audio> доиграет без UI).
  useEffect(() => {
    const el = audioRef.current;
    return () => {
      el?.pause();
    };
  }, [sample?.src]);

  const stop = () => {
    audioRef.current?.pause();
    setPlaying(false);
  };

  const toggle = () => {
    const el = audioRef.current;
    if (!el || !sample) return;
    if (playing) {
      stop();
      return;
    }
    el.currentTime = sample.start;
    void el.play().then(
      () => setPlaying(true),
      () => setPlaying(false),
    );
  };

  const onTime = () => {
    const el = audioRef.current;
    if (!el || !sample) return;
    if (el.currentTime >= sample.end) stop();
  };

  return (
    <>
      {sample && (
        <audio
          ref={audioRef}
          src={sample.src}
          preload="none"
          onTimeUpdate={onTime}
          onEnded={stop}
          onPause={() => setPlaying(false)}
        />
      )}
      <IconBtn
        icon={playing ? 'pause' : 'play'}
        size="sm"
        label={playing ? t('speakers.sampleStopAria') : t('speakers.samplePlayAria')}
        disabled={!sample}
        onClick={toggle}
      />
    </>
  );
}
