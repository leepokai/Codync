import type { Bot } from '@shared/models'
import type { HostClient } from '../client/host-client'

type Perform = (op: (c: HostClient) => Promise<unknown>, undo: () => void) => Promise<boolean>

/** One computer's roster read marks: "Mark as read" and "Mark as Unread", and the hold that keeps a chat unread on screen. */
export class ReadMarks {
  private bots: () => Map<string, Bot>
  private changed: () => void
  private perform: Perform
  /** Mark as Unread sent but not yet showing as the bot's `keptUnread`: a bot event from before it mustn't drop the hold. */
  private marking = new Map<string, Promise<boolean>>()
  constructor(bots: () => Map<string, Bot>, changed: () => void, perform: Perform) {
    this.bots = bots
    this.changed = changed
    this.perform = perform
  }

  /** Whether automatic reads leave this chat alone. */
  held(botId: string) {
    return this.marking.has(botId) || !!this.bots().get(botId)?.keptUnread
  }

  /** A bot event: once it shows the host's hold, that carries it. */
  received(bot: Bot) {
    if (bot.keptUnread) this.marking.delete(bot.id)
  }

  /** Sent after a Mark as Unread still under way, so the host never gets them the other way round. */
  markRead(botId: string) {
    const bot = this.bots().get(botId)
    const marking = this.marking.get(botId)
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
    let kept = true
    const sent = this.perform(async (c) => {
      kept = (await c.call<{ bot?: Bot }>('markUnread', { botId })).bot?.keptUnread !== false
    }, () => {})
    this.marking.set(botId, sent)
    void sent.then((accepted) => {
      if ((!accepted || !kept) && this.marking.get(botId) === sent) this.marking.delete(botId)
    })
  }

  /** The user opened the chat: one marked unread is read now. */
  open(botId: string) {
    if (this.held(botId)) this.markRead(botId)
  }

  private patch(botId: string, change: Partial<Bot>) {
    const bot = this.bots().get(botId)
    if (!bot) return
    this.bots().set(botId, { ...bot, ...change })
    this.changed()
  }
}
