import type { Entry } from '@shared/models'
import { mergeTaskEntries, unsentEntries } from '@shared/tasks'

/**
 * Background tasks' transcripts by task id, held while a task's details are open (`watch`); once closed, only the
 * messages that haven't reached the thread yet, so none is lost before it arrives.
 */
export class TaskTranscripts {
  private held = new Map<string, Entry[]>()
  /** How many views hold each of them open. */
  private watchers = new Map<string, number>()

  /** Holds a task's transcript from now until the returned function is called: its details are open. */
  watch(taskId: string): () => void {
    if (!this.held.has(taskId)) this.held.set(taskId, [])
    this.watchers.set(taskId, (this.watchers.get(taskId) ?? 0) + 1)
    return () => {
      // The inspector and its sheet for a narrow window can both show a thread for a moment.
      const left = (this.watchers.get(taskId) ?? 1) - 1
      if (left > 0) return void this.watchers.set(taskId, left)
      this.watchers.delete(taskId)
      this.hold(taskId, this.of(taskId))
    }
  }

  /** A held task's transcript, oldest first. */
  of(taskId: string): Entry[] {
    return this.held.get(taskId) ?? []
  }

  /** More of a task's transcript (fetched, or a live `taskEntry`); dropped when nothing holds the task. */
  merge(taskId: string, entries: Entry[]) {
    const held = this.held.get(taskId)
    if (held) this.hold(taskId, mergeTaskEntries(held, entries))
  }

  /** All of a watched task's transcript; of a closed one, only its unsent messages. */
  private hold(taskId: string, entries: Entry[]) {
    if (this.watchers.has(taskId)) return void this.held.set(taskId, entries)
    const unsent = unsentEntries(entries)
    if (unsent.length) this.held.set(taskId, unsent)
    else this.held.delete(taskId)
  }
}
