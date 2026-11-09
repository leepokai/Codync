import type { Bot } from '@shared/models'
import type { HostClient } from '../client/host-client'
import { isBelow } from '../../shared/compat.ts'

/** The first host with `markUnread`. */
export const MARK_UNREAD_HOST = '2.44.0'

/** The roster offers "Mark as Unread" for a chat with nothing unread; otherwise, or on an older host (it refuses `markUnread`), "Mark as Read". */
export function offersMarkUnread(bot: Bot, hostVersion: string | undefined) {
  return bot.unread <= 0 && !bot.keptUnread && hostVersion !== undefined && !isBelow(hostVersion, MARK_UNREAD_HOST)
}

type Operation = (c: HostClient) => Promise<unknown>
type Perform = (op: Operation, undo: () => void) => Promise<boolean>

/** What the read marks need from the store they keep marks for. */
export interface ReadMarksHost {
  bots(): Map<string, Bot>
  changed(): void
  /** Stores the bot a host's answer carries. */
  put(bot: Bot): void
  /** Sends `op`: what fails reaches the dialog, after `undo`. */
  perform: Perform
  /** Sends `op`, rejecting when it fails: nothing reaches the dialog. */
  quietly(op: Operation): Promise<unknown>
}

/** One computer's read marks: opening a chat, the roster's "Mark as read" and "Mark as Unread". The host decides what a read does to a chat marked unread; here they only go out in order, and show at once. */
export class ReadMarks {
  private readonly host: ReadMarksHost
  /** Each chat's last read mark on its way, retries included: the next waits for it, so the host gets them in the order they were made. */
  private sending = new Map<string, Promise<boolean>>()
  /** What each chat showed before the reads on their way: a second read mustn't take the first's optimistic state for it. */
  private beforeReads = new Map<string, Bot>()
  /** Chats whose last open never reached the host: they're read as opened once they're on screen with a link (`retryOpen`). */
  private unsentOpens = new Set<string>()
  constructor(host: ReadMarksHost) {
    this.host = host
  }

  /** The roster's "Mark as read". */
  markRead(botId: string) {
    this.read(botId, this.host.perform)
  }

  /** The user opened the chat, which reads it whatever is known here of it. A failure never reaches the dialog. */
  open(botId: string) {
    this.read(botId, async (op, undo) => {
      try {
        await this.host.quietly(op)
        return true
      } catch {
        undo()
        this.unsentOpens.add(botId)
        return false
      }
    })
  }

  /** The chat is on screen with a link: true when that sent an open that hadn't reached the host. */
  retryOpen(botId: string) {
    if (!this.unsentOpens.has(botId)) return false
    this.open(botId)
    return true
  }

  /** The roster's "Mark as Unread". */
  markUnread(botId: string) {
    if (!this.host.bots().has(botId)) return
    let marked: Bot | undefined
    void this.inOrder(botId, this.host.perform, async (c) => {
      marked = (await c.call<{ bot: Bot }>('markUnread', { botId })).bot
    }, () => {}).then(() => {
      // A newer bot event may have beaten the answer here.
      const now = this.host.bots().get(botId)
      if (marked && now && now.rev < marked.rev) this.host.put(marked)
    })
  }

  /** Shows the chat read at once; a refusal gives back what was there, unless a newer bot event from the host replaced it. */
  private read(botId: string, perform: Perform) {
    const bot = this.host.bots().get(botId)
    if (!bot) return
    const kept = this.beforeReads.get(botId)
    const before = kept?.rev === bot.rev ? kept : bot
    this.beforeReads.set(botId, before)
    this.patch(botId, { unread: 0, keptUnread: false })
    void this.inOrder(botId, perform, (c) => c.call('markRead', { botId }), () => {
      if (this.host.bots().get(botId)?.rev === before.rev) this.patch(botId, { unread: before.unread, keptUnread: before.keptUnread })
    })
  }

  /** A message sent from here, which reads the chat on the host: it goes out after the chat's marks on their way, retries included, and the next mark waits for it. */
  send<T>(botId: string, deliver: () => Promise<T>): Promise<T> {
    const before = this.sending.get(botId)
    const delivered = before ? before.then(deliver) : deliver()
    this.queue(botId, delivered.then(() => true, () => false))
    return delivered
  }

  private inOrder(botId: string, perform: Perform, op: Operation, undo: () => void) {
    this.unsentOpens.delete(botId)
    const before = this.sending.get(botId)
    // `perform`'s retry window starts when it's called, so it waits for the mark before it to start.
    const sent = before
      ? before.then(() => {
        // An open before this mark may have failed while it waited: this one is newer.
        this.unsentOpens.delete(botId)
        return perform(op, undo)
      })
      : perform(op, undo)
    return this.queue(botId, sent)
  }

  private queue(botId: string, sent: Promise<boolean>) {
    this.sending.set(botId, sent)
    void sent.then(() => {
      if (this.sending.get(botId) !== sent) return
      this.sending.delete(botId)
      this.beforeReads.delete(botId)
    })
    return sent
  }

  private patch(botId: string, change: Partial<Bot>) {
    const bot = this.host.bots().get(botId)
    if (!bot) return
    this.host.bots().set(botId, { ...bot, ...change })
    this.host.changed()
  }
}
