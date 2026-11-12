import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readFileSync } from 'node:fs'

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

test("every What's new note is translated in every catalog", () => {
  const announcements: { version: string; notes: string[] }[] = JSON.parse(readFileSync(new URL('../Announcements/announcements.json', directory), 'utf8'))
  for (const locale of ['zh-Hans', 'zh-Hant', 'ja', 'ko', 'fr', 'de', 'es', 'pt-BR', 'ru', 'it']) {
    const catalog = read(locale)
    for (const note of announcements.flatMap((a) => a.notes)) assert.ok(Object.hasOwn(catalog, note), `${locale}: ${note}`)
  }
})
