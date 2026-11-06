import type { CustomIcons } from '@shared/custom-icons'

/** Images stay out of localStorage's small quota and the transcript cache. One entry per computer/account. */
async function database(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open('codync-icons-v1', 1)
    request.onupgradeneeded = () => request.result.createObjectStore('icons')
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
  })
}

export async function loadIconCache(key: string): Promise<CustomIcons> {
  const db = await database()
  try {
    return await new Promise((resolve, reject) => {
      const request = db.transaction('icons').objectStore('icons').get(key)
      request.onsuccess = () => resolve((request.result as CustomIcons | undefined) ?? {})
      request.onerror = () => reject(request.error)
    })
  } finally { db.close() }
}

async function writeIconCache(key: string, icons: CustomIcons): Promise<void> {
  const db = await database()
  try {
    await new Promise<void>((resolve, reject) => {
      const transaction = db.transaction('icons', 'readwrite')
      transaction.objectStore('icons').put(icons, key)
      transaction.oncomplete = () => resolve()
      transaction.onerror = () => reject(transaction.error)
      transaction.onabort = () => reject(transaction.error ?? new Error('Icon cache write was cancelled'))
    })
  } finally { db.close() }
}

// Preserve replacement/removal order even when opening IndexedDB finishes out of order.
const pending = new Map<string, Promise<void>>()

export function saveIconCache(key: string, icons: CustomIcons): Promise<void> {
  const snapshot = structuredClone(icons)
  const previous = pending.get(key) ?? Promise.resolve()
  const next = previous.catch(() => {}).then(() => writeIconCache(key, snapshot))
  pending.set(key, next)
  const cleanup = () => { if (pending.get(key) === next) pending.delete(key) }
  void next.then(cleanup, cleanup)
  return next
}
