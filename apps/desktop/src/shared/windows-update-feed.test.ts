import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import { parse, stringify } from 'yaml'

const require = createRequire(import.meta.url)
const { mergeWindowsUpdateFeeds } = require('../../tools/windows-update-feed.mjs')
const { findFile } = require('electron-updater/out/providers/Provider.js')

const feed = (arch: string, version = '2.33.0') => ({
  version,
  files: [{ url: `codync-windows-${arch}.exe`, sha512: `${arch}-hash`, size: 123 }],
  path: `codync-windows-${arch}.exe`,
  sha512: `${arch}-hash`,
  releaseDate: '2026-10-29T00:00:00.000Z',
})

test('one Windows feed keeps both installers and legacy x64 metadata', () => {
  const x64 = feed('x64')
  const arm64 = feed('arm64')
  const merged = mergeWindowsUpdateFeeds(x64, arm64)
  assert.deepEqual(merged, { ...x64, files: [...x64.files, ...arm64.files] })
  assert.equal(x64.files.length, 1)
})

for (const arch of ['x64', 'arm64'] as const) {
  test(`electron-updater selects the ${arch} installer from the combined feed`, (context) => {
    context.mock.property(process, 'arch', arch)
    const merged = mergeWindowsUpdateFeeds(feed('x64'), feed('arm64'))
    const resolved = merged.files.map((info: { url: string }) => ({ info, url: new URL(info.url, 'https://example.com/') }))
    assert.equal(findFile(resolved, 'exe').info.url, `codync-windows-${arch}.exe`)
  })
}

test('rejects different versions and missing, swapped or unhashed installers', () => {
  assert.throws(() => mergeWindowsUpdateFeeds(feed('x64'), feed('arm64', '2.32.0')), /versions must match/)
  assert.throws(() => mergeWindowsUpdateFeeds(feed('x64'), { ...feed('arm64'), files: [] }), /must describe/)
  assert.throws(() => mergeWindowsUpdateFeeds(feed('arm64'), feed('x64')), /must describe/)
  const unhashed = { ...feed('arm64'), files: [{ url: 'codync-windows-arm64.exe', size: 123 }] }
  assert.throws(() => mergeWindowsUpdateFeeds(feed('x64'), unhashed), /SHA-512/)
})

test('the workflow command merges YAML feeds on disk', (context) => {
  const dir = mkdtempSync(join(tmpdir(), 'windows-update-feed-'))
  context.after(() => rmSync(dir, { recursive: true, force: true }))
  const x64 = join(dir, 'x64.yml')
  const arm64 = join(dir, 'arm64.yml')
  const output = join(dir, 'latest.yml')
  writeFileSync(x64, stringify(feed('x64')))
  writeFileSync(arm64, stringify(feed('arm64')))
  const script = fileURLToPath(new URL('../../tools/windows-update-feed.mjs', import.meta.url))
  execFileSync(process.execPath, [script, x64, arm64, output])
  const merged = parse(readFileSync(output, 'utf8'))
  assert.deepEqual(merged, mergeWindowsUpdateFeeds(feed('x64'), feed('arm64')))
})
