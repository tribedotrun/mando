/**
 * Playwright config for Electron integration tests.
 *
 * Run: npx playwright test --config=playwright.daemon.config.ts
 * Prereq: cargo build --manifest-path rust/Cargo.toml --bin mando-gw --bin mando-cc-mock && npm run build:test
 */
import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/integration',
  timeout: 60_000,
  retries: 0,
  workers: 4,
  fullyParallel: true,
  reporter: 'list',
  use: {
    trace: 'retain-on-failure',
  },
});
