import { execFileSync } from 'node:child_process'
import { resolve } from 'node:path'
import react from '@vitejs/plugin-react'
import { defineConfig, externalizeDepsPlugin } from 'electron-vite'

// Fresh macOS checkouts must include SF Symbols without a separate manual icons step.
if (process.platform === 'darwin') {
  execFileSync('swift', [resolve('tools/export-symbols.swift')], { stdio: 'inherit' })
}

export default defineConfig({
  main: { plugins: [externalizeDepsPlugin()] },
  preload: { plugins: [externalizeDepsPlugin()] },
  renderer: {
    resolve: { alias: { '@shared': resolve('src/shared') } },
    plugins: [react()],
  },
})
