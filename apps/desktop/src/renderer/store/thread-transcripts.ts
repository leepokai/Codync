import type { Entry } from '@shared/models'
import { mergeThreadEntries, unsentEntries } from '@shared/threads'

/**
 * Threads' transcripts by thread id, held while a thread's details are open (`watch`); once closed, only the
 * messages that haven't reached the thread yet, so none is lost before it arrives.
 */
export class ThreadTranscripts {
  private held = new Map<string, Entry[]>()
  /** How many views hold each of them open. */
  private watchers = new Map<string, number>()

  /** Holds a thread's transcript from now until the returned function is called: its details are open. */
  watch(threadId: string): () => void {
    if (!this.held.has(threadId)) this.held.set(threadId, [])
    this.watchers.set(threadId, (this.watchers.get(threadId) ?? 0) + 1)
    return () => {
      // The inspector and its sheet for a narrow window can both show a thread for a moment.
      const left = (this.watchers.get(threadId) ?? 1) - 1
      if (left > 0) return void this.watchers.set(threadId, left)
      this.watchers.delete(threadId)
      this.hold(threadId, this.of(threadId))
    }
  }

  /** A held thread's transcript, oldest first. */
  of(threadId: string): Entry[] {
    return this.held.get(threadId) ?? []
  }

  /** More of a thread's transcript (fetched, or a live `threadEntry`); dropped when nothing holds the thread. */
  merge(threadId: string, entries: Entry[]) {
    const held = this.held.get(threadId)
    if (held) this.hold(threadId, mergeThreadEntries(held, entries))
  }

  /** All of a watched thread's transcript; of a closed one, only its unsent messages. */
  private hold(threadId: string, entries: Entry[]) {
    if (this.watchers.has(threadId)) return void this.held.set(threadId, entries)
    const unsent = unsentEntries(entries)
    if (unsent.length) this.held.set(threadId, unsent)
    else this.held.delete(threadId)
  }
}
