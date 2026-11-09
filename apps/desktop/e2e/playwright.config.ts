import { defineConfig } from '@playwright/test'

// Desktop e2e attaches to this worktree's running sandbox (`npm run dev -- --cdp`) instead of
// launching Electron itself: the sandbox owns the host, data and sign-in, and one app runs per
// worktree, so one worker here. Worktrees run in parallel with each other.
export default defineConfig({
  testDir: '.',
  testMatch: '**/*.spec.ts',
  workers: 1,
  timeout: 30_000,
  reporter: [['list']],
  outputDir: '../test-results',
  use: { trace: 'retain-on-failure' },
})
