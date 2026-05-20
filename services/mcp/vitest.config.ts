import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    globals: false,
    coverage: {
      provider: 'v8',
      reporter: ['text', 'lcov'],
      reportsDirectory: './coverage',
      include: ['src/**/*.ts'],
      exclude: ['src/**/*.d.ts', 'src/server.ts'],
      thresholds: {
        lines: 40,
        statements: 40,
        functions: 50,
        branches: 60,
      },
    },
  },
});
