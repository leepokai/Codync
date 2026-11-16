import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const source = fileURLToPath(new URL('../..', import.meta.url))
const directory = new URL('../../../../ios/Kit/Sources/CodyncKit/Resources/Languages/', import.meta.url)
const languages = ['zh-Hans', 'zh-Hant', 'ja', 'ko', 'fr', 'de', 'es', 'pt-BR', 'ru', 'it']
const call = /\btr\(\s*(['"`])((?:\\[\s\S]|(?!\1)[\s\S])*?)\1/g

function files(dir: string): string[] {
  const found: string[] = []
  for (const name of readdirSync(dir)) {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) found.push(...files(path))
    else if (/\.(ts|tsx)$/.test(name) && !/\.test\.(ts|tsx)$/.test(name)) found.push(path)
  }
  return found
}

function keys(): Map<string, string> {
  const found = new Map<string, string>()
  for (const file of files(source)) {
    for (const match of readFileSync(file, 'utf8').matchAll(call)) {
      const raw = match[2]
      // A template literal with an expression is computed, not a catalog key.
      if (raw.includes('${')) continue
      const text = raw.replace(/\\'/g, "'").replace(/\\"/g, '"').replace(/\\n/g, '\n').replace(/\\\\/g, '\\')
      if (text.trim() && !found.has(text)) found.set(text, file)
    }
  }
  return found
}

test('every literal tr() key exists in every shipped catalog', () => {
  const literal = keys()
  // A broken scan would find almost nothing and pass silently; the desktop has hundreds.
  assert.ok(literal.size > 400, `the scan found ${literal.size} literal tr() keys`)
  const catalogs: Record<string, Record<string, string>> = {}
  for (const language of languages) {
    catalogs[language] = JSON.parse(readFileSync(new URL(`${language}.json`, directory), 'utf8'))
  }
  const missing: string[] = []
  for (const [key, file] of literal) {
    for (const language of languages) {
      if (!Object.hasOwn(catalogs[language], key)) {
        missing.push(`${language}: ${JSON.stringify(key)} (${relative(source, file)})`)
      }
    }
  }
  assert.deepEqual(missing, [], 'a tr() key no catalog carries falls back to the English text in every language')
})
