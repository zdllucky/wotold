// [perm-usage] Секция разрешений раньше не имела тестов вовсе, а вся её суть —
// клей поверх одной Tauri-команды: что показать при каком статусе и что дёрнуть
// по кнопке. Правило 1 («клей тестируется первым») здесь и нарушалось.
// Мок Tauri поднимается до импорта секции — канон SettingsPage.test.

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';

const invoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: unknown) => invoke(cmd, args) as unknown,
}));

import { ToastProvider } from '../ui';
import { ThemeProvider } from '../theme/useTheme';
import { PermissionsSection } from './PermissionsSection';

/** Ответ `get_audio_permissions` без строки accessibility — как отдаёт Rust. */
function status(microphone: string, screen: string) {
  return { microphone, screen_recording: screen };
}

/**
 * Бэкенд как состояние, а не как очередь ответов: секция опрашивает статус и
 * при монтировании, и при возврате фокуса, поэтому `mockResolvedValueOnce`
 * зависит от числа опросов и разъезжается от любой правки эффектов.
 */
function mockBackend(initial: ReturnType<typeof status>) {
  let current = initial;
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'request_audio_permissions') {
      current = status('granted', 'granted');
      return current;
    }
    if (cmd === 'get_audio_permissions') return current;
    return undefined;
  });
  return {
    /** Имитирует выдачу доступа в System Settings — мимо приложения. */
    grantOutside: () => {
      current = status('granted', 'granted');
    },
  };
}

function renderSection() {
  return render(
    <ThemeProvider>
      <ToastProvider>
        <PermissionsSection />
      </ToastProvider>
    </ThemeProvider>,
  );
}

describe('PermissionsSection', () => {
  beforeEach(() => invoke.mockReset());

  it('строки «Универсальный доступ» нет — AX не требуется ни одной фиче', async () => {
    mockBackend(status('granted', 'granted'));
    renderSection();

    expect(await screen.findAllByText('выдано')).toHaveLength(2);
    expect(screen.queryByText('Универсальный доступ')).toBeNull();
  });

  it('запрос микрофона идёт с target=microphone и обновляет статус', async () => {
    mockBackend(status('not_determined', 'granted'));
    renderSection();

    await screen.findByText('не запрошено');
    const [micRequest] = screen.getAllByRole('button', { name: 'Запросить' });
    fireEvent.click(micRequest as HTMLElement);

    await waitFor(() => expect(screen.getAllByText('выдано')).toHaveLength(2));
    expect(invoke).toHaveBeenCalledWith('request_audio_permissions', {
      target: 'microphone',
    });
  });

  // Разрешение выдают в System Settings, о чём приложению никто не сообщает.
  // Без пере-опроса пользователь возвращался в окно с прежним «отказано».
  it('возврат фокуса в окно перечитывает статус', async () => {
    const backend = mockBackend(status('denied', 'granted'));
    renderSection();

    await screen.findByText('отказано');
    backend.grantOutside();
    fireEvent.focus(window);

    await waitFor(() => expect(screen.getAllByText('выдано')).toHaveLength(2));
  });

  // ad-hoc подпись (R6) роняет TCC-грант на каждом обновлении: галочка в
  // Системных настройках остаётся, а доступ пропадает, и «Запросить» не
  // помогает — macOS второй раз диалог для принятого решения не показывает.
  it('при denied предлагает сброс TCC и после подтверждения запрашивает заново', async () => {
    mockBackend(status('denied', 'granted'));
    renderSection();

    await screen.findByText('отказано');
    fireEvent.click(screen.getByRole('button', { name: 'Сбросить доступ: Микрофон' }));
    fireEvent.click(screen.getByRole('button', { name: 'Сбросить и запросить' }));

    await waitFor(() => expect(screen.getAllByText('выдано')).toHaveLength(2));
    expect(invoke).toHaveBeenCalledWith('reset_permission', { pane: 'microphone' });
    expect(invoke).toHaveBeenLastCalledWith('request_audio_permissions', {
      target: 'microphone',
    });
  });

  // При двух отказанных строках получались две кнопки с одинаковым доступным
  // именем — скринридер их не различал, и диалог не говорил, что именно сбросит.
  it('кнопки и диалог сброса называют разрешение', async () => {
    mockBackend(status('denied', 'denied'));
    renderSection();

    await screen.findAllByText('отказано');
    expect(screen.getByRole('button', { name: 'Сбросить доступ: Микрофон' })).toBeTruthy();
    const screenReset = screen.getByRole('button', {
      name: 'Сбросить доступ: Захват системного звука',
    });

    fireEvent.click(screenReset);
    expect(
      screen.getByText('Сбросить доступ и запросить заново: Захват системного звука'),
    ).toBeTruthy();
  });

  it('сброс не предлагается, пока разрешение не отказано', async () => {
    mockBackend(status('not_determined', 'granted'));
    renderSection();

    await screen.findByText('не запрошено');
    expect(screen.queryByRole('button', { name: /Сбросить доступ/ })).toBeNull();
  });

  // Системный диалог сам забирает и возвращает фокус, поэтому «Запросить» и
  // фоновый опрос идут внахлёст. Опрос читает состояние ДО ответа
  // пользователя — применить его значит откатить свежевыданное разрешение.
  it('фоновый опрос не откатывает результат явного запроса', async () => {
    // Порядок здесь и есть предмет теста: устаревший опрос обязан завершиться
    // ПОСЛЕ запроса — только в этом порядке он и способен перетереть статус.
    let releaseRequest: (() => void) | undefined;
    let releasePoll: (() => void) | undefined;
    let polls = 0;

    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'request_audio_permissions') {
        await new Promise<void>((resolve) => {
          releaseRequest = resolve;
        });
        return status('granted', 'granted');
      }
      if (cmd !== 'get_audio_permissions') return undefined;
      polls += 1;
      if (polls === 1) return status('denied', 'granted'); // опрос при монтировании
      await new Promise<void>((resolve) => {
        releasePoll = resolve;
      });
      return status('denied', 'granted'); // состояние ДО ответа пользователя
    });
    renderSection();

    await screen.findByText('отказано');

    // Окно получило фокус — опрос пошёл и завис на медленном сайдкаре.
    fireEvent.focus(window);
    await waitFor(() => expect(releasePoll).toBeDefined());

    // Пока он висит, пользователь жмёт «Запросить» и отвечает на диалог.
    fireEvent.click(screen.getByRole('button', { name: 'Запросить' }));
    await waitFor(() => expect(releaseRequest).toBeDefined());
    releaseRequest?.();
    await waitFor(() => expect(screen.getAllByText('выдано')).toHaveLength(2));

    // И только теперь долетает устаревший опрос — с состоянием ДО ответа.
    releasePoll?.();
    await waitFor(() => expect(polls).toBe(2));
    expect(screen.getAllByText('выдано')).toHaveLength(2);
    expect(screen.queryByText('отказано')).toBeNull();
  });

  // Красный алерт переживал свою причину: разрешение выдали в System Settings,
  // чипы позеленели, а ошибка продолжала висеть сверху.
  it('успешный фоновый опрос убирает ошибку', async () => {
    let failing = true;
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd !== 'get_audio_permissions') return undefined;
      if (failing) throw new Error('permissions sidecar terminated: signal 6');
      return status('granted', 'granted');
    });
    renderSection();

    await screen.findByRole('alert');
    failing = false;
    fireEvent.focus(window);

    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
  });

  // Fail-path: сайдкар умер, не прислав события. Сообщение обязано быть
  // переведённым, а не сырой строкой бэкенда.
  it('падение проверки показывает переведённую ошибку', async () => {
    // Роняем только опрос разрешений: общий reject ловил бы и соседей по
    // дереву (ThemeProvider читает настройку той же командой invoke).
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_audio_permissions') {
        throw new Error('permissions sidecar terminated: signal 6');
      }
      return undefined;
    });
    renderSection();

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toMatch(/не удалось проверить разрешения/i);
    expect(alert.textContent).not.toMatch(/terminated/i);
  });
});
