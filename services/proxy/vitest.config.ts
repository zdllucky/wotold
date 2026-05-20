import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    globals: false,
    // *.integration.test.ts бегут в отдельном workers-pool проекте — см. vitest.workers.config.ts.
    exclude: ['**/node_modules/**', '**/dist/**', 'src/**/*.integration.test.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html', 'lcov'],
      reportsDirectory: './coverage',
      include: ['src/**/*.ts'],
      exclude: ['src/**/*.d.ts'],
      thresholds: {
        // Baseline после #19 backfill. Целим 60% к концу [B7] follow-up.
        lines: 20,
        statements: 20,
        functions: 25,
        branches: 55,
      },
    },
  },
});
