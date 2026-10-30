import type { Bot } from '@shared/models'
import type { HostClient } from '../client/host-client'

type Perform = (op: (c: HostClient) => Promise<unknown>, undo: () => void) => Promise<boolean>

/** One computer's roster read marks: "Mark as read" and "Mark as Unread" shown at once, then sent; a refusal takes them back. */
export class ReadMarks {
  private bots: () => Map<string, Bot>
  private changed: () => void
  private perform: Perform
  constructor(bots: () => Map<string, Bot>, changed: () => void, perform: Perform) {
    this.bots = bots
    this.changed = changed
    this.perform = perform
  }

  markRead(botId: string) {
    const bot = this.bots().get(botId)
    if (!bot || (bot.unread <= 0 && !bot.keptUnread)) return
    this.patch(botId, { unread: 0, keptUnread: false })
    void this.perform((c) => c.call('markRead', { botId }), () => this.patch(botId, { keptUnread: bot.keptUnread }))
  }

  /** The host counts what comes back unread; until then only the hold shows, so nothing on screen reads it. */
  markUnread(botId: string) {
    if (!this.bots().has(botId)) return
    this.patch(botId, { keptUnread: true })
    void this.perform((c) => c.call('markUnread', { botId }), () => this.patch(botId, { keptUnread: false }))
  }

  /** The user opened the chat: one marked unread is read now. */
  open(botId: string) {
    if (this.bots().get(botId)?.keptUnread) this.markRead(botId)
  }

  private patch(botId: string, change: Partial<Bot>) {
    const bot = this.bots().get(botId)
    if (!bot) return
    this.bots().set(botId, { ...bot, ...change })
    this.changed()
  }
}
