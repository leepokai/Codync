/** Drafts saved before 2.20.1 were keyed `["<bot>",null]`; a thread's (`["<bot>","<root>"]`) is dropped. */
function chatOf(key: string): string | null {
  let old: unknown
  try { old = JSON.parse(key) } catch { return key }
  if (!Array.isArray(old)) return key
  return old.length === 2 && typeof old[0] === 'string' && old[1] === null ? old[0] : null
}

/** Text belongs to a chat, independent of a mounted editor or transcript cache. */
export class ComposerDrafts {
  private values = new Map<string, string>()
  private listeners = new Set<() => void>()
  private retired = false

  private storage: Pick<Storage, 'getItem' | 'setItem'>
  private key: string
  private onSaveError?: () => void

  constructor(storage: Pick<Storage, 'getItem' | 'setItem'>, key: string, onSaveError?: () => void) {
    this.storage = storage
    this.key = key
    this.onSaveError = onSaveError
    try {
      const saved: unknown = JSON.parse(storage.getItem(key) ?? 'null')
      if (Array.isArray(saved)) {
        for (const row of saved) {
          if (Array.isArray(row) && row.length === 2 && typeof row[0] === 'string' && typeof row[1] === 'string') {
            const chat = chatOf(row[0])
            if (chat) this.values.set(chat, row[1])
          }
        }
      }
    } catch { /* A damaged or unavailable store must not prevent typing. */ }
  }

  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => { this.listeners.delete(listener) }
  }

  get(bot: string) {
    return this.values.get(bot) ?? ''
  }

  set(bot: string, text: string) {
    if (this.retired) return
    if (text) this.values.set(bot, text)
    else this.values.delete(bot)
    this.save()
  }

  /** The chat's draft and its artifacts' (`artifactDraftKey`). */
  removeBot(bot: string) {
    if (this.retired) return
    for (const key of [...this.values.keys()]) if (key === bot || key.startsWith(`${bot}/`)) this.values.delete(key)
    this.save()
  }

  clear() {
    if (this.retired) return
    this.values.clear()
    this.save()
  }

  retire() {
    this.retired = true
  }

  private save() {
    // Save on each edit, including clearing after submission; no unmount/debounce race.
    try { this.storage.setItem(this.key, JSON.stringify([...this.values])) } catch { this.onSaveError?.() }
    for (const listener of this.listeners) listener()
  }
}
