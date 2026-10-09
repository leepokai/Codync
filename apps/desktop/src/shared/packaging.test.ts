import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
const require = createRequire(import.meta.url)
const dev = require('../../tools/dev-builder.cjs')
const { AppInfo } = require('app-builder-lib/out/appInfo')
const { getWindowsInstallationDirName, getWindowsInstallationAppPackageName } = require('app-builder-lib/out/targets/targetUtil')
const { parse } = require('yaml')
const prod = parse(readFileSync(new URL('../../electron-builder.yml', import.meta.url), 'utf8'))
const metadata = JSON.parse(readFileSync(new URL('../../package.json', import.meta.url), 'utf8'))

test('Linux packages and Windows installation resources have separate development names', () => {
  const mainInfo = new AppInfo({ config: prod, metadata })
  const devInfo = new AppInfo({ config: dev, metadata: { ...metadata, ...dev.extraMetadata } })
  assert.equal(mainInfo.linuxPackageName, 'codync-desktop')
  assert.equal(devInfo.linuxPackageName, 'codync-desktop-dev')
  assert.notEqual(devInfo.updaterCacheDirName, mainInfo.updaterCacheDirName)
  assert.notEqual(getWindowsInstallationDirName(devInfo, true), getWindowsInstallationDirName(mainInfo, true))
  assert.notEqual(getWindowsInstallationAppPackageName(devInfo.name), getWindowsInstallationAppPackageName(mainInfo.name))
  assert.equal(dev.extraMetadata.desktopName, 'codync-dev.desktop')
})

test('development packaging registers only development callbacks and background helpers', () => {
  assert.equal(dev.appId, 'com.pokai.Codync.dev')
  assert.equal(dev.productName, 'Codync Dev')
  assert.equal(dev.publish, null)
  assert.ok(new RegExp(dev.mac.signIgnore[0]).test('CodyncDevScreen.app/Contents/Helpers/cua-driver'))
  assert.deepEqual(dev.protocols.flatMap((p: { schemes: string[] }) => p.schemes), ['codync-dev', 'com.pokai.Codync.dev'])
  assert.deepEqual(dev.mac.extraFiles.map((f: { to: string }) => f.to), [
    'Library/LoginItems/CodyncDevScreen.app',
    'Library/LaunchAgents/com.pokai.Codync.dev.screen.plist',
  ])
  assert.ok(dev.win.extraResources.some((f: { to: string }) => f.to === 'codync-dev-hostw.exe'))
  assert.ok(!dev.win.extraResources.some((f: { to: string }) => f.to === 'codync-hostw.exe'))
  assert.equal(dev.linux.executableName, 'codync-dev')
})
