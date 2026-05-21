import '@testing-library/jest-dom/vitest';
import { afterEach } from 'vitest';
import { cleanup } from '@testing-library/react';

afterEach(() => {
  cleanup();
});

// Pin navigator.language to ru-RU so i18n falls back to Russian translations
// during component tests. Existing tests assert against ru strings; keep that
// stable independently of host environment locale.
if (typeof navigator !== 'undefined') {
  try {
    Object.defineProperty(navigator, 'language', {
      configurable: true,
      get: () => 'ru-RU',
    });
  } catch {
    /* navigator.language may be non-configurable on some platforms */
  }
}

// jsdom does not implement scrollIntoView — stub it globally.
if (typeof window !== 'undefined') {
  window.HTMLElement.prototype.scrollIntoView = function () {};

  // jsdom does not implement matchMedia — stub it so ThemeProvider works.
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}
