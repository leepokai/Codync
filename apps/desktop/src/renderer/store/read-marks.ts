import type { Bot } from '@shared/models'
import type { HostClient } from '../client/host-client'

type Perform = (op: (c: HostClient) => Promise<unknown>, undo: () => void) => Promise<boolean>

/** A Mark as Unread on its way; `from` is the host's rev for it once it answered. */
interface Marking {
  sent: Promise<boolean>
  from?: number
}

/** One computer's roster read marks: "Mark as read" and "Mark as Unread", and the hold that keeps a chat unread on screen. */
export class ReadMarks {
  private bots: () => Map<string, Bot>
  private changed: () => void
  private perform: Perform
  /** Mark as Unread sent but not yet seen in a bot event: one from before it mustn't drop the hold. */
  private marking = new Map<string, Marking>()
  /** Chats opened before the host's state arrived: a hold it shows then was still the user's to read. */
  private openedEarly: Set<string> | null = new Set()
  constructor(bots: () => Map<string, Bot>, changed: () => void, perform: Perform) {
    this.bots = bots
    this.changed = changed
    this.perform = perform
  }

  /** Whether automatic reads leave this chat alone. */
  held(botId: string) {
    return this.marking.has(botId) || !!this.bots().get(botId)?.keptUnread
  }

  /** A bot event: once it shows the host's hold, or is from after the mark, the host's state carries it. */
  received(bot: Bot) {
    const marking = this.marking.get(bot.id)
    if (marking && (bot.keptUnread || bot.rev >= (marking.from ?? Infinity))) this.marking.delete(bot.id)
  }

  /** Sent after a Mark as Unread still under way, so the host never gets them the other way round. */
  markRead(botId: string) {
    const bot = this.bots().get(botId)
    const marking = this.marking.get(botId)?.sent
    if (!bot || (bot.unread <= 0 && !this.held(botId))) return
    this.marking.delete(botId)
    this.patch(botId, { unread: 0, keptUnread: false })
    void this.perform(async (c) => {
      await marking
      return c.call('markRead', { botId })
    }, () => this.patch(botId, { keptUnread: bot.keptUnread }))
  }

  markUnread(botId: string) {
    if (!this.bots().has(botId)) return
    let held: Bot | undefined
    const sent = this.perform(async (c) => {
      held = (await c.call<{ bot?: Bot }>('markUnread', { botId })).bot
    }, () => {})
    const marking: Marking = { sent }
    this.marking.set(botId, marking)
    void sent.then((accepted) => {
      if (this.marking.get(botId) !== marking) return
      if (!accepted || held?.keptUnread === false) {
        this.marking.delete(botId)
        return
      }
      marking.from = held?.rev
      const now = this.bots().get(botId)
      if (now) this.received(now)
    })
  }

  /** The user opened the chat: one marked unread is read now, or once the host's state arrives. */
  open(botId: string) {
    if (this.held(botId)) this.markRead(botId)
    else this.openedEarly?.add(botId)
  }

  /** A new events stream: what's here may be behind the host until it catches up. */
  catchingUp() {
    this.openedEarly ??= new Set()
  }

  /** The stream reached the host's rev: chats opened meanwhile that it holds unread are read. */
  caughtUp() {
    const opened = this.openedEarly ?? []
    this.openedEarly = null
    for (const botId of opened) if (this.held(botId)) this.markRead(botId)
  }

  private patch(botId: string, change: Partial<Bot>) {
    const bot = this.bots().get(botId)
    if (!bot) return
    this.bots().set(botId, { ...bot, ...change })
    this.changed()
  }
}
