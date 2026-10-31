import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test, type TestContext } from 'node:test'

const require = createRequire(import.meta.url)
const verifyBuild = require('../../tools/verify-build.cjs')
const { Arch } = require('electron-builder')

function fixture(t: TestContext, platform = 'darwin', arch = Arch.x64) {
  const projectDir = mkdtempSync(join(tmpdir(), 'codync-verify-build-'))
  t.after(() => rmSync(projectDir, { recursive: true, force: true }))
  mkdirSync(join(projectDir, 'resources'))
  writeFileSync(join(projectDir, 'resources/account-config.json'), JSON.stringify({ environment: 'dev' }))
  if (platform === 'win32') writeHost(projectDir, 0x8664)
  return { electronPlatformName: platform, arch, packager: { projectDir, appInfo: { id: 'com.pokai.Codync.dev' } } }
}

/** A bundled Windows host with only the PE header fields the check reads. */
function writeHost(projectDir: string, machine: number) {
  const exe = Buffer.alloc(0x100)
  exe.writeUInt32LE(0x80, 0x3c)
  exe.write('PE\0\0', 0x80, 'latin1')
  exe.writeUInt16LE(machine, 0x84)
  mkdirSync(join(projectDir, 'build/native'), { recursive: true })
  writeFileSync(join(projectDir, 'build/native/codync-host.exe'), exe)
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

test('Windows packaging refuses a host built for the other architecture', async (t) => {
  const arm = fixture(t, 'win32', Arch.arm64)
  await assert.rejects(verifyBuild(arm), /Windows arm64 app with a host built for x64/)
  writeHost(arm.packager.projectDir, 0xaa64)
  await assert.doesNotReject(verifyBuild(arm))
})
