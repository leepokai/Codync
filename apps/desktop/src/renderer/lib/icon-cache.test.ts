import { strict as assert } from 'node:assert'
import { test } from 'node:test'
import { loadIconCache, saveIconCache } from './icon-cache.ts'

test('disk cache keeps the newest replacement and removal, isolated by account/computer', async () => {
  const values = new Map<string, unknown>()
  let failNextWrite = false
  const original = globalThis.indexedDB
  // Delay the first open: independent writes would otherwise allow an old image to win.
  let opens = 0
  globalThis.indexedDB = { open() {
    const request: any = {}
    const delay = opens++ === 0 ? 30 : 0
    setTimeout(() => {
      request.result = {
        close() {},
        transaction(_name: string, _mode?: string) {
          const transaction: any = { objectStore() { return {
            put(value: unknown, key: string) {
              setTimeout(() => {
                if (failNextWrite) {
                  failNextWrite = false
                  transaction.error = new Error('Disk full')
                  transaction.onerror()
                } else {
                  values.set(key, value)
                  transaction.oncomplete()
                }
              }, 0)
            },
            get(key: string) {
              const read: any = {}
              setTimeout(() => { read.result = values.get(key); read.onsuccess() }, 0)
              return read
            },
          } } }
          return transaction
        },
      }
      request.onsuccess()
    }, delay)
    return request
  } } as unknown as IDBFactory
  try {
    const first = { 'bot:b': { dataUrl: 'first', tinted: false } }
    const a = saveIconCache('alice/computer', first)
    first['bot:b'].dataUrl = 'mutated after enqueue'
    const b = saveIconCache('alice/computer', { 'bot:b': { dataUrl: 'second', tinted: true } })
    const other = saveIconCache('bob/computer', { 'bot:b': { dataUrl: 'other account', tinted: false } })
    await Promise.all([a, b, other])
    assert.equal((await loadIconCache('alice/computer'))['bot:b']?.dataUrl, 'second')
    assert.equal((await loadIconCache('bob/computer'))['bot:b']?.dataUrl, 'other account')
    failNextWrite = true
    const failed = saveIconCache('alice/computer', first)
    const removed = saveIconCache('alice/computer', {})
    await assert.rejects(failed, /Disk full/)
    await removed
    assert.deepEqual(await loadIconCache('alice/computer'), {})
  } finally { globalThis.indexedDB = original }
})
