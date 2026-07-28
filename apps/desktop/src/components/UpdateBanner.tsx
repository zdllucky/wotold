// Баннер обновления.
//
// Проверку он больше не делает: она живёт в Rust (`spawn_updater_poll`) и
// приезжает событием. Раньше проверка сидела в useEffect этого компонента —
// один раз за запуск, ошибка молча в console.warn. Приложение для записи
// звонков открыто сутками, и узнавать о новой версии только при следующем
// холодном старте недостаточно.
//
// Баннер остался ровно для одного случая: обязательное обновление ждёт, пока
// закончится запись или обработка. Об этом нужно сказать явно — иначе
// перезапуск «сам по себе» через двадцать минут выглядит сбоем. Всё
// необязательное показывается тостом с кнопкой.
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';

import { humanError } from '../api/errors';
import { type AvailableUpdate, UPDATER_AVAILABLE_EVENT, applyUpdate } from '../api/updater';
import { useI18n } from '../i18n';
import { useToast } from '../ui/Toast';

export function UpdateBanner() {
  const { t } = useI18n();
  const toast = useToast();
  const [pending, setPending] = useState<AvailableUpdate | null>(null);

  useEffect(() => {
    const unlisten = listen<AvailableUpdate>(UPDATER_AVAILABLE_EVENT, (event) => {
      const update = event.payload;

      if (update.urgency === 'mandatory') {
        // Установку уже поставил в очередь Rust — она ждёт простоя. Здесь
        // только объясняем пользователю предстоящий перезапуск.
        setPending(update);
        return;
      }

      toast.show({
        message: t('update.toastAvailable', { version: update.version }),
        action: {
          label: t('update.toastAction'),
          onClick: () => {
            void applyUpdate().catch((e: unknown) => {
              toast.show({ message: humanError(e, t), tone: 'danger' });
            });
          },
        },
      });
    });

    return () => {
      void unlisten.then((off) => off());
    };
  }, [t, toast]);

  if (!pending) return null;

  return (
    <div
      className="panel panel--raised update-banner"
      role="status"
    >
      <p className="update-banner-title">
        {t('update.mandatoryPending', { version: pending.version })}
      </p>
      <p className="u-faint update-banner-hint">
        {t('update.mandatoryPendingHint')}
      </p>
    </div>
  );
}
