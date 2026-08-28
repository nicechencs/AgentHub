import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { defineConfig, devices } from '@playwright/test';

/**
 * Browser smoke against `pnpm dev:mock` only.
 * Uses a dedicated e2ePort so a local `pnpm dev` / Tauri Vite is not reused.
 * Does not cover Tauri, real accounts, or production adapter selection.
 */
const runtime = JSON.parse(
  readFileSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), 'scripts/dev-runtime.json'), 'utf8'),
) as { host: string; e2ePort: number };
const PORT = runtime.e2ePort;
const HOST = runtime.host;
const baseURL = `http://${HOST}:${PORT}`;

export default defineConfig({
  testDir: './e2e/browser',
  fullyParallel: false,
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  timeout: 60_000,
  expect: { timeout: 12_000 },
  reporter: process.env.CI
    ? [['list'], ['html', { open: 'never', outputFolder: 'playwright-report' }]]
    : [['list']],
  outputDir: 'test-results',
  use: {
    baseURL,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'off',
    locale: 'zh-CN',
  },
  webServer: {
    command: `pnpm exec vite --mode mock --host ${HOST} --port ${PORT} --strictPort`,
    url: baseURL,
    reuseExistingServer: false,
    timeout: 120_000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
