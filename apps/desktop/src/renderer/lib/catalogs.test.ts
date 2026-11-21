import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { announcements, phrases } from './whats-new.ts'

const directory = new URL('../../../../ios/Kit/Sources/CodyncKit/Resources/Languages/', import.meta.url)
const languages = readdirSync(directory).filter((name) => name.endsWith('.json')).map((name) => name.slice(0, -'.json'.length))
const text = (language: string): string => readFileSync(new URL(`${language}.json`, directory), 'utf8')
const read = (language: string): Record<string, string> => JSON.parse(text(language))

/** Every key the file spells out, in order, including a repeat a parser would drop. */
const spelledKeys = (text: string): string[] => {
  const keys: string[] = []
  let at = text.indexOf('{') + 1
  while (at > 0 && at < text.length) {
    while (at < text.length && /[\s,]/.test(text[at])) at++
    if (text[at] !== '"') return keys
    const starts = at
    at++
    while (at < text.length && text[at] !== '"') at += text[at] === '\\' ? 2 : 1
    keys.push(JSON.parse(text.slice(starts, at + 1)) as string)
    at = text.indexOf(':', at) + 1
    while (at < text.length && /\s/.test(text[at])) at++
    if (text[at] !== '"') throw new Error('a catalog value is not a string')
    at++
    while (at < text.length && text[at] !== '"') at += text[at] === '\\' ? 2 : 1
    at++
  }
  return keys
}

test('no catalog spells a key twice, so a translation is never dropped', () => {
  for (const locale of languages) {
    const keys = spelledKeys(text(locale))
    const repeated = keys.filter((key, index) => keys.indexOf(key) !== index)
    assert.deepEqual(repeated, [], `${locale}: the last one wins, so "${repeated.at(-1) ?? ''}" loses its other translation`)
  }
})

test('all shipped catalogs have the same keys and preserve interpolation tokens', () => {
  const source = read('zh-Hans')
  for (const locale of languages.filter((language) => language !== 'zh-Hans')) {
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
  for (const locale of languages) {
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
    for (const [, , literal] of text.matchAll(/\b(?:tr|trNow|sshMessage)\((['"`])((?:\\.|(?!\1|\$\{)[^\\\n])+)\1[,)]/g)) {
      keys.add(literal.replace(/\\(.)/g, (_, char: string) => (char === 'n' ? '\n' : char)))
    }
  }
  for (const locale of languages) {
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

// Interface text that has a catalog entry but reaches JSX as a bare string renders English in every
// language: a text node, or a visible attribute (placeholder, tooltip, accessible name) of a DOM element.
// Component props are not checked (the shared components pass their title / label / placeholder through
// tr()); component text is, except for Button, Switch and AccessNotice, which translate their string children.
const TRANSLATES_CHILDREN = new Set(['AccessNotice', 'Button', 'Switch'])
/** An opening tag (attribute values nest braces two deep) and any text that follows it, even across lines. */
const TAG = /<([A-Za-z][\w.]*)((?:[^<>{}]|\{(?:[^{}]|\{[^{}]*\})*\})*?)(\/?)>(?:([^<>{}]+)(?=<|$))?/g
const DOM_TEXT_ATTRIBUTE = /(?<![\w-])(?:placeholder|title|aria-label|alt)="([^"]+)"/g

function untranslated(source: string, phrases: Set<string>): { found: { label: string; line: number }[]; nodes: number } {
  const found: { label: string; line: number }[] = []
  let nodes = 0
  for (const match of source.matchAll(TAG)) {
    const [, name, attributes, selfClosing, text] = match
    const labels = /^[a-z]/.test(name) ? [...attributes.matchAll(DOM_TEXT_ATTRIBUTE)].map((attribute) => attribute[1]) : []
    if (text && !selfClosing) {
      nodes += 1
      if (!TRANSLATES_CHILDREN.has(name)) labels.push(text.replace(/\s+/g, ' ').trim())
    }
    const line = source.slice(0, match.index).split('\n').length
    for (const label of labels) if (phrases.has(label)) found.push({ label, line })
  }
  return { found, nodes }
}

test('a bare catalog phrase in JSX is found across lines, in nested markup and in DOM attributes', () => {
  const phrases = new Set(['Delete group chat', 'Search'])
  const labels = (source: string) => untranslated(source, phrases).found.map((text) => text.label)
  assert.deepEqual(labels('<button>\n  Delete group chat\n</button>'), ['Delete group chat'])
  assert.deepEqual(labels('<div>\n  <span>Delete group chat</span>\n</div>'), ['Delete group chat'])
  assert.deepEqual(labels('<input value={q} onChange={(e) => set({ q: e.target.value })} placeholder="Search" />'), ['Search'])
  assert.deepEqual(labels('<IconButton title="Search" /><Button>Search</Button>'), [])
})

test('no desktop JSX text or DOM attribute shows a catalog phrase without tr()', () => {
  const src = new URL('../../', import.meta.url)
  const files = readdirSync(src, { recursive: true, encoding: 'utf8' }).filter((path) => path.endsWith('.tsx'))
  const phrases = new Set(Object.keys(read('zh-Hans')))
  let nodes = 0
  const missing: string[] = []
  for (const file of files) {
    const scan = untranslated(readFileSync(new URL(file, src), 'utf8'), phrases)
    nodes += scan.nodes
    for (const text of scan.found) missing.push(`${file}:${text.line} ${JSON.stringify(text.label)}`)
  }
  assert.ok(nodes > 500, `expected to scan JSX text nodes, scanned ${nodes}`)
  assert.deepEqual(missing, [], 'wrap these in tr() or add a catalog entry')
})
