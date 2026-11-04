import { defineConfig } from '@playwright/test';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  testDir: 'tests',
  globalSetup: './global-setup.mjs',
  workers: 1,
  fullyParallel: false,
  timeout: 60_000,
  reporter: [['list']],
  outputDir: 'test-results',
  use: {
    baseURL: pathToFileURL(join(here, '.standalone', 'index.html')).href,
    viewport: { width: 1100, height: 620 },
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
    launchOptions: {
      // Set CHROME_PATH to use a locally installed Chrome instead of Playwright's bundled build.
      executablePath: process.env.CHROME_PATH || undefined,
      args: ['--no-sandbox', '--disable-gpu', '--allow-file-access-from-files'],
    },
  },
});
