import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test, type TestContext } from 'node:test'

const verifyBuild = createRequire(import.meta.url)('../../tools/verify-build.cjs')

function fixture(t: TestContext, platform = 'darwin') {
  const projectDir = mkdtempSync(join(tmpdir(), 'codync-symbol-check-'))
  t.after(() => rmSync(projectDir, { recursive: true, force: true }))
  mkdirSync(join(projectDir, 'resources'))
  writeFileSync(join(projectDir, 'resources/account-config.json'), JSON.stringify({ environment: 'dev' }))
  return { electronPlatformName: platform, packager: { projectDir, appInfo: { id: 'com.pokai.Codync.dev' } } }
}

test('macOS packaging requires the Apple logo manifest entry and every rendered weight', async (t) => {
  const context = fixture(t)
  await assert.rejects(verifyBuild(context), /ENOENT/)
  const symbols = join(context.packager.projectDir, 'out/renderer/symbols')
  mkdirSync(symbols, { recursive: true })
  writeFileSync(join(symbols, 'manifest.json'), '{}')
  await assert.rejects(verifyBuild(context), /missing the Apple logo/)
  writeFileSync(join(symbols, 'manifest.json'), JSON.stringify({ 'apple.logo': { w: 0.9, h: 1.15 } }))
  await assert.rejects(verifyBuild(context), /ENOENT/)
  for (const weight of ['light', 'regular', 'medium', 'semibold', 'bold', 'black']) {
    writeFileSync(join(symbols, `apple.logo.${weight}.png`), 'fixture')
  }
  await assert.doesNotReject(verifyBuild(context))
})

test('Linux and Windows packaging does not require Apple-only SF Symbols', async (t) => {
  for (const platform of ['linux', 'win32']) {
    await assert.doesNotReject(verifyBuild(fixture(t, platform)))
  }
})
