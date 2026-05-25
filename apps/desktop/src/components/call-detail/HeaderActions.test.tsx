// [P1.3] HeaderActions vitest — verify «Пересоздаём…» rendering с/без
// elapsed timer'а из RecapProgressEvent payload.

import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test } from 'vitest';

import { HeaderActions } from './HeaderActions';

const baseProps = {
  onReprocess: () => {},
  onRegenerateRecap: () => {},
  onRegenerateTitle: () => {},
  onExport: () => {},
  onDelete: () => {},
  reprocessing: false,
  regenerating: false,
  regenerateDisabled: false,
  regeneratingTitle: false,
  regenerateTitleDisabled: false,
  exporting: false,
  deleting: false,
};

async function openMenu() {
  const trigger = screen.getByRole('button', { name: /Действия|Actions/i });
  await act(async () => {
    trigger.click();
  });
}

describe('HeaderActions — recap regen elapsed', () => {
  afterEach(() => cleanup());

  test('idle state shows regenerate button label', async () => {
    render(<HeaderActions {...baseProps} />);
    await openMenu();
    expect(screen.getByText(/Пересоздать саммари|Regenerate recap/i)).toBeInTheDocument();
  });

  test('regenerating without elapsed shows fallback «Пересоздаём…»', async () => {
    render(<HeaderActions {...baseProps} regenerating={true} />);
    await openMenu();
    expect(screen.getByText(/^Пересоздаём…$|^Regenerating…$/)).toBeInTheDocument();
  });

  test('regenerating with elapsed=15 shows «Пересоздаём… 15s»', async () => {
    render(<HeaderActions {...baseProps} regenerating={true} recapElapsedSec={15} />);
    await openMenu();
    expect(screen.getByText(/Пересоздаём… 15s|Regenerating… 15s/)).toBeInTheDocument();
  });

  test('regenerating with elapsed=0 still renders elapsed variant (not fallback)', async () => {
    // Edge case: elapsed=0 (точно после первого tick), Number is truthy для !== null.
    render(<HeaderActions {...baseProps} regenerating={true} recapElapsedSec={0} />);
    await openMenu();
    expect(screen.getByText(/Пересоздаём… 0s|Regenerating… 0s/)).toBeInTheDocument();
  });
});
