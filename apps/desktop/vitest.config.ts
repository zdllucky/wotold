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
        // [TD-34] Ratchet, а не «цель». Порог стоял на 10% при фактических
        // 72% — можно было удалить четыре пятых тестов, и CI остался бы
        // зелёным, то есть гейт не защищал ничего.
        //
        // Правило: «факт минус 3 п.п.» — запас на удаление мёртвого кода,
        // который может просесть покрытием, но не на удаление тестов.
        // Поднимать при каждом заметном приросте; понижать — только по явному
        // согласованию (CLAUDE.md, раздел про тестирование).
        //
        // Замер 2026-07-27: lines/statements 72.12, functions 61.59,
        // branches 85.52.
        lines: 69,
        statements: 69,
        functions: 58,
        branches: 82,
      },
    },
  },
});
