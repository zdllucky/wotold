import { defineWorkersConfig } from '@cloudflare/vitest-pool-workers/config';

// vitest-pool-workers конфиг для integration-тестов через миниframe.
// Подбирает только *.integration.test.ts — обычные unit-тесты бегут в default node config.

export default defineWorkersConfig({
  test: {
    include: ['src/**/*.integration.test.ts'],
    poolOptions: {
      workers: {
        wrangler: { configPath: './wrangler.test.toml' },
      },
    },
  },
});
