import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

// Interface text that exists in a language catalog but reaches JSX as a bare string is never
// translated: the app renders English in every language. This guards the renderer against it.
const renderer = fileURLToPath(new URL('..', import.meta.url))
const languages = fileURLToPath(new URL('../../../../ios/Kit/Sources/CodyncKit/Resources/Languages', import.meta.url))

const phrases = new Set<string>()
for (const name of readdirSync(languages)) {
  if (!name.endsWith('.json')) continue
  for (const key of Object.keys(JSON.parse(readFileSync(join(languages, name), 'utf8')) as Record<string, string>)) phrases.add(key)
}

/** Controls that already pass a string child through tr(); checking their text would be redundant. */
const TRANSLATES_CHILDREN = new Set(['AccessNotice', 'Button', 'Switch'])
/** An opening tag and the text that follows it, even when that text is split across lines. */
const TEXT_NODE = /<([A-Za-z][\w.]*)((?:[^<>{}]|\{[^{}]*\})*?)(\/?)>([^<>{}]+)(?=<|$)/g

/** Text nodes whose label has a catalog entry but that never reach tr(). */
export function untranslated(source: string, count?: { nodes: number }): { label: string; line: number }[] {
  const found: { label: string; line: number }[] = []
  for (const match of source.matchAll(TEXT_NODE)) {
    if (match[3] === '/') continue
    if (count) count.nodes += 1
    if (TRANSLATES_CHILDREN.has(match[1])) continue
    const label = match[4].replace(/\s+/g, ' ').trim()
    if (label && phrases.has(label)) found.push({ label, line: source.slice(0, match.index).split('\n').length })
  }
  return found
}

function sources(dir: string): string[] {
  const found: string[] = []
  for (const name of readdirSync(dir)) {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) found.push(...sources(path))
    else if (name.endsWith('.tsx') && !name.includes('.test.')) found.push(path)
  }
  return found
}

test('a label split across lines is still found', () => {
  assert.deepEqual(untranslated('<button>\n  Delete group chat\n</button>').map((text) => text.label), ['Delete group chat'])
})

test('a label inside nested markup is still found', () => {
  assert.deepEqual(untranslated('<div>\n  <span>Delete group chat</span>\n</div>').map((text) => text.label), ['Delete group chat'])
})

test('no JSX text is left untranslated when a catalog has the phrase', () => {
  const files = sources(renderer)
  const nodes = { nodes: 0 }
  const missing: string[] = []
  for (const path of files) {
    const source = readFileSync(path, 'utf8')
    for (const text of untranslated(source, nodes)) missing.push(`${path.slice(renderer.length)}:${text.line} ${JSON.stringify(text.label)}`)
  }
  assert.ok(files.length > 100, `expected to scan the renderer, scanned ${files.length} files`)
  assert.ok(nodes.nodes > 500, `expected to scan JSX text nodes, scanned ${nodes.nodes}`)
  assert.deepEqual(missing, [], 'wrap these in tr() or add a catalog entry')
})
