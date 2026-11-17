import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { announcements, phrases } from './whats-new.ts'

const directory = new URL('../../../../ios/Kit/Sources/CodyncKit/Resources/Languages/', import.meta.url)
const read = (language: string): Record<string, string> => JSON.parse(readFileSync(new URL(`${language}.json`, directory), 'utf8'))

test('all shipped catalogs have the same keys and preserve interpolation tokens', () => {
  const source = read('zh-Hans')
  for (const locale of ['zh-Hant', 'ja', 'ko', 'fr', 'de', 'es', 'pt-BR', 'ru', 'it']) {
    const catalog = read(locale)
    assert.equal(catalog.English, "English", `${locale}: English keeps its native name in language menus`)
    assert.deepEqual(Object.keys(catalog).sort(), Object.keys(source).sort(), locale)
    for (const [key, value] of Object.entries(catalog)) {
      assert.ok(value.trim(), `${locale}: ${key}`)
      assert.deepEqual((value.match(/\{[A-Za-z0-9]+\}/g) ?? []).sort(), (key.match(/\{[A-Za-z0-9]+\}/g) ?? []).sort(), `${locale}: ${key}`)
      assert.equal((value.match(/%d/g) ?? []).length, (key.match(/%d/g) ?? []).length, `${locale}: ${key} (%d)`)
      assert.doesNotMatch(value, /ZXQ\d|QXZ/, `${locale}: ${key}`)
    }
  }
})

test("every What's new phrase is translated in every catalog, and the clips beside the file are the ones it names, each under 300 KB", () => {
  const folder = new URL('../../../CodyncUI/Resources/Announcements/', directory)
  const clips = readdirSync(folder).filter((name) => name.endsWith('.mp4'))
  // Every app ships the whole folder: a clip no feature names is dead weight in all of them.
  assert.deepEqual(clips.sort(), announcements.flatMap((a) => a.features ?? []).flatMap((f) => (f.media ? `${f.media}.mp4` : [])).sort())
  for (const clip of clips) assert.ok(statSync(new URL(clip, folder)).size < 300_000, clip)
  for (const locale of ['zh-Hans', 'zh-Hant', 'ja', 'ko', 'fr', 'de', 'es', 'pt-BR', 'ru', 'it']) {
    const catalog = read(locale)
    for (const note of phrases(announcements)) assert.ok(Object.hasOwn(catalog, note), `${locale}: ${note}`)
  }
})

test('every literal tr() / trNow() phrase in the desktop source has a catalog entry (#485)', () => {
  const src = new URL('../../', import.meta.url)
  const files = readdirSync(src, { recursive: true, encoding: 'utf8' }).filter((path) => /\.tsx?$/.test(path) && !path.endsWith('.test.ts'))
  const keys = new Set<string>()
  for (const file of files) {
    const text = readFileSync(new URL(file, src), 'utf8')
    for (const [, , literal] of text.matchAll(/\btr(?:Now)?\((['"`])((?:\\.|(?!\1|\$\{)[^\\\n])+)\1[,)]/g)) {
      keys.add(literal.replace(/\\(.)/g, (_, char: string) => (char === 'n' ? '\n' : char)))
    }
  }
  for (const locale of ['zh-Hans', 'zh-Hant', 'ja', 'ko', 'fr', 'de', 'es', 'pt-BR', 'ru', 'it']) {
    const catalog = read(locale)
    assert.deepEqual([...keys].filter((key) => !Object.hasOwn(catalog, key)).sort(), [], locale)
  }
})

test('every literal phrase the iPhone, Android and terminal apps translate has a catalog entry (#488)', () => {
  const repo = new URL('../../../../../', import.meta.url)
  const sources: [string, RegExp, RegExp][] = [
    ['apps/ios', /\.swift$/, /\bL10n\.tr\("((?:\\.|[^"\\\n])+)"/g],
    ['apps/android/app/src/main', /\.kt$/, /\btr\("((?:\\.|[^"\\\n])+)"\)/g],
    ['host/src/tui', /\.rs$/, /\btr\("((?:\\.|[^"\\\n])+)"\)/g],
  ]
  const catalog = read('zh-Hans')
  const missing: string[] = []
  for (const [folder, file, call] of sources) {
    const root = new URL(`${folder}/`, repo)
    for (const path of readdirSync(root, { recursive: true, encoding: 'utf8' }).filter((path) => file.test(path) && !/Tests?\b|_tests\.rs$/.test(path))) {
      for (const [, literal] of readFileSync(new URL(path, root), 'utf8').matchAll(call)) {
        // Interpolated at run time, or no words to translate (a bullet, a cron line, the product name).
        if (/\\\(|\$/.test(literal) || !/[a-z]/.test(literal) || literal === 'Codync') continue
        const key = literal.replace(/\\(.)/g, (_, char: string) => (char === 'n' ? '\n' : char))
        // Each client retries a miss trimmed.
        if (!Object.hasOwn(catalog, key) && !Object.hasOwn(catalog, key.trim())) missing.push(`${folder}/${path}: ${key}`)
      }
    }
  }
  assert.deepEqual(missing.sort(), [])
})
