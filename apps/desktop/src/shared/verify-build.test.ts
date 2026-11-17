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
  const root = mkdtempSync(join(tmpdir(), 'codync-verify-build-'))
  const projectDir = join(root, 'desktop')
  t.after(() => rmSync(root, { recursive: true, force: true }))
  mkdirSync(projectDir)
  mkdirSync(join(projectDir, 'resources'))
  writeFileSync(join(projectDir, 'resources/account-config.json'), JSON.stringify({ environment: 'dev' }))
  mkdirSync(join(root, 'browser-extension'))
  writeFileSync(join(root, 'browser-extension/manifest.json'), JSON.stringify({
    manifest_version: 3,
    version: '0.1.0',
    permissions: ['debugger', 'nativeMessaging'],
  }))
  if (platform === 'win32') writeHosts(projectDir, 0x8664)
  return { electronPlatformName: platform, arch, packager: { projectDir, appInfo: { id: 'com.pokai.Codync.dev' } } }
}

/** A bundled Windows host with only the PE header fields the check reads. */
function writeHosts(projectDir: string, machine: number) {
  const exe = Buffer.alloc(0x100)
  exe.writeUInt32LE(0x80, 0x3c)
  exe.write('PE\0\0', 0x80, 'latin1')
  exe.writeUInt16LE(machine, 0x84)
  mkdirSync(join(projectDir, 'build/native'), { recursive: true })
  for (const name of ['codync-host.exe', 'codync-browser-native-host.exe']) {
    writeFileSync(join(projectDir, `build/native/${name}`), exe)
  }
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
  await assert.rejects(verifyBuild(arm), /Windows arm64 app with codync-host\.exe built for x64/)
  writeHosts(arm.packager.projectDir, 0xaa64)
  await assert.doesNotReject(verifyBuild(arm))
})

test('Windows packaging verifies the native helper and browser extension', async (t) => {
  const context = fixture(t, 'win32', Arch.x64)
  writeFileSync(join(context.packager.projectDir, 'build/native/codync-browser-native-host.exe'), Buffer.alloc(0x100))
  await assert.rejects(verifyBuild(context), /codync-browser-native-host\.exe built for unknown/)

  writeHosts(context.packager.projectDir, 0x8664)
  writeFileSync(
    join(context.packager.projectDir, '../browser-extension/manifest.json'),
    JSON.stringify({ manifest_version: 2, version: '0.1.0', permissions: ['debugger', 'nativeMessaging'] }),
  )
  await assert.rejects(verifyBuild(context), /Manifest V3/)

  writeFileSync(
    join(context.packager.projectDir, '../browser-extension/manifest.json'),
    JSON.stringify({ manifest_version: 3, version: '0.1.0', permissions: ['debugger'] }),
  )
  await assert.rejects(verifyBuild(context), /native bridge or browser control permission/)
})
