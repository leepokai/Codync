import assert from 'node:assert/strict'
import { existsSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'

registerHooks({ resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('.') && context.parentURL && !/\.[a-z]+$/.test(specifier)) {
    const url = new URL(`${specifier}.ts`, context.parentURL)
    if (existsSync(url)) return nextResolve(url.href, context)
  }
  return nextResolve(specifier, context)
} })

const rows = (from: number, to: number) => Array.from({ length: to - from }, (_, i) => from + i)

test('more connectors read on past empty pages and stop at a failure', async () => {
  const { ConnectorCatalog } = await import('./connector-catalog.ts')
  const asked: (string | null)[] = []
  let answer = async (cursor: string | null): Promise<{ items: number[]; next: string | null }> => {
    asked.push(cursor)
    if (cursor === null) return { items: rows(0, 15), next: 'a' }
    return cursor === 'a' ? { items: [], next: 'b' } : { items: rows(15, 25), next: 'c' }
  }
  const catalog = new ConnectorCatalog<number>(String, (_search, cursor) => answer(cursor))
  await catalog.load('')
  assert.equal(catalog.items.items.length, 15)

  // The next page has nothing to install but moves the cursor on: the one after it brings the rows.
  await catalog.loadMore()
  assert.equal(catalog.items.items.length, 15)
  assert.equal(catalog.cursor, 'b')
  await catalog.loadMore()
  assert.deepEqual(asked, [null, 'a', 'b'])
  assert.equal(catalog.items.items.length, 25)
  assert.equal(catalog.cursor, 'c')

  answer = async () => { throw new Error('offline') }
  await catalog.loadMore()
  assert.equal(catalog.items.items.length, 25)
  assert.equal(catalog.cursor, 'c')
  assert.equal(catalog.error, 'offline')
  assert.equal(catalog.loadingMore, false)

  // The cursor ends: nothing more to ask for.
  answer = async () => ({ items: rows(25, 26), next: null })
  await catalog.loadMore()
  assert.equal(catalog.items.items.length, 26)
  assert.equal(catalog.hasMore, false)
  assert.equal(catalog.error, null)
})
