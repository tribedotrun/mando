import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  timeout: 30000,
  retries: 0,
  workers: 4,
  fullyParallel: true,
  use: {
    trace: 'on-first-retry',
  },
});
