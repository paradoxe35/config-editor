import { defineConfig } from '@playwright/test';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));

// The suite runs in parallel: 4 workers locally, 2 on CI's 4-CPU runners
// (E2E_WORKERS to change it). The timed specs always run alone, after the
// others, so their numbers stay comparable from run to run.
const workers = Number(process.env.E2E_WORKERS) || (process.env.CI ? 2 : 4);

// Specs that measure time (latency budgets on large documents): they run
// after the others, one at a time, so parallel load does not skew them.
const TIMED = ['**/large-files.spec.mjs', '**/insert-text-burst.spec.mjs', '**/problems-timing.spec.mjs', '**/large-samples.spec.mjs', '**/folding.spec.mjs', '**/search.spec.mjs'];

export default defineConfig({
  testDir: 'tests',
  globalSetup: './global-setup.mjs',
  workers,
  timeout: 60_000,
  reporter: [['list']],
  outputDir: 'test-results',
  projects: [
    { name: 'editor', testIgnore: TIMED, fullyParallel: workers > 1 },
    { name: 'timed', testMatch: TIMED, dependencies: ['editor'], workers: 1 },
  ],
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
