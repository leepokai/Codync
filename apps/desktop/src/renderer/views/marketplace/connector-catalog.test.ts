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

test('a connector page fills until it is full, and falls back when the registry fails', async () => {
  const { ConnectorCatalog } = await import('./connector-catalog.ts')
  const asked: (string | null)[] = []
  let answer = async (cursor: string | null) => {
    asked.push(cursor)
    if (cursor === null) return { items: rows(0, 15), next: 'a' }
    return cursor === 'a' ? { items: rows(15, 18), next: 'b' } : { items: rows(18, 25), next: 'c' }
  }
  const catalog = new ConnectorCatalog<number>(String, (_search, cursor) => answer(cursor))
  await catalog.load('')
  assert.equal(catalog.items.items.length, 15)

  // Page 2 needs rows 10-19: two registry pages fill it.
  catalog.show(1, 10)
  await catalog.fill(10)
  assert.deepEqual(asked, [null, 'a', 'b'])
  assert.equal(catalog.page, 1)
  assert.equal(catalog.items.items.length, 25)
  assert.equal(catalog.cursor, 'c')

  // The page past the loaded rows fails: back to the last page with rows.
  answer = async () => { throw new Error('offline') }
  catalog.show(3, 10)
  assert.equal(catalog.page, 3)
  await catalog.fill(10)
  assert.equal(catalog.page, 2)
  assert.equal(catalog.error, 'offline')
  assert.equal(catalog.loadingMore, false)
})
