import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: false,
    setupFiles: ['./src/test/setup.ts'],
    css: true,
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html', 'lcov'],
      reportsDirectory: './coverage',
      include: ['src/**/*.{ts,tsx}'],
      exclude: [
        'src/**/*.d.ts',
        'src/main.tsx',
        'src/dev-tauri-mock.ts',
        'src/test/**',
        'src/api/**',
        'src/pages/DesignSystemPage.tsx',
      ],
      thresholds: {
        // Baseline. Поднимать постепенно — целим 80% к концу [B7] follow-up.
        lines: 10,
        statements: 10,
        functions: 10,
        branches: 60,
      },
    },
  },
});
