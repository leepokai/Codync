// Lists the literal keys the desktop calls tr('…') with that a language file lacks.
// Usage: node tools/i18n-audit.mjs [language] [--strict]
import { readdir, readFile } from 'node:fs/promises'
import { join } from 'node:path'

const language = process.argv[2] && !process.argv[2].startsWith('--') ? process.argv[2] : 'es'
const strict = process.argv.includes('--strict')
const root = 'apps/desktop/src/renderer'
const catalogPath = `apps/ios/Kit/Sources/CodyncKit/Resources/Languages/${language}.json`

async function files(dir) {
  const entries = await readdir(dir, { withFileTypes: true })
  const found = []
  for (const entry of entries) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) found.push(...(await files(path)))
    else if (/\.(tsx?|jsx?)$/.test(entry.name) && !entry.name.includes('.test.')) found.push(path)
  }
  return found
}

const catalog = JSON.parse(await readFile(catalogPath, 'utf8'))
const used = new Map()
const keyPattern = /\btr\(\s*(['"])((?:(?!\1).){2,160})\1\s*\)/g
for (const path of await files(root)) {
  const text = await readFile(path, 'utf8')
  for (const match of text.matchAll(keyPattern)) {
    // A source `\n` inside tr('…') is a newline at runtime, which is how the catalog stores it.
    const key = match[2].replaceAll('\\n', '\n')
    if (!used.has(key)) used.set(key, path)
  }
}

const missing = [...used].filter(([key]) => !(key in catalog))
console.log(`${used.size} literal keys in ${root}; ${missing.length} missing from ${language}.json.`)
for (const [key, path] of missing.slice(0, 200)) console.log(`  ${JSON.stringify(key)} (${path.replace(/\\/g, '/')})`)
if (missing.length > 200) console.log(`  … and ${missing.length - 200} more`)
process.exit(strict && missing.length > 0 ? 1 : 0)
