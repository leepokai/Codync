import type { HostClient } from '../client/host-client'
import { Observable } from '../lib/observable.ts'
import type { DiscordStatus, SlackStatus, TelegramBotStatus } from '../../shared/channels'

interface TelegramReply { bots: TelegramBotStatus[]; result?: { code?: string; expiresAt?: number } }

/** The messaging channels of this computer's host: Telegram bots (add, pair, allow) and the Slack connection. */
export class ChannelsStore extends Observable {
  telegram: TelegramBotStatus[] = []
  slack: SlackStatus | null = null
  discord: DiscordStatus | null = null
  loading = false
  busy = false
  error: string | null = null
  /** The pairing code just made, shown once. */
  pairing: { name: string; code: string } | null = null
  private active = true
  private readonly client: () => Promise<Pick<HostClient, 'call'>>
  constructor(client: () => Promise<Pick<HostClient, 'call'>>) { super(); this.client = client }
  resume() { this.active = true }
  dispose() { this.active = false; this.loading = false; this.busy = false }

  async load() {
    if (!this.active || this.loading) return
    this.loading = true; this.error = null; this.changed()
    try {
      const host = await this.client()
      const [telegram, slack, discord] = await Promise.all([
        host.call<TelegramReply>('telegramStatus', {}),
        host.call<SlackStatus>('slackStatus', {}),
        host.call<DiscordStatus>('discordStatus', {}),
      ])
      if (!this.active) return
      this.telegram = telegram.bots
      this.slack = slack
      this.discord = discord
    } catch (error) { if (this.active) this.error = String(error) }
    finally { if (this.active) { this.loading = false; this.changed() } }
  }

  /** Runs one change and keeps the answer the host gives. */
  private async act(method: string, body: Record<string, unknown>): Promise<TelegramReply | null> {
    if (!this.active || this.busy) return null
    this.busy = true; this.error = null; this.changed()
    try {
      const reply = await (await this.client()).call<TelegramReply>(method, body)
      if (this.active) this.telegram = reply.bots
      return reply
    } catch (error) { if (this.active) this.error = String(error); return null }
    finally { if (this.active) { this.busy = false; this.changed() } }
  }

  async addTelegram(name: string, token: string, botId: string) {
    return (await this.act('addTelegramBot', { name, token, botId })) !== null
  }
  async removeTelegram(name: string) {
    if (await this.act('removeTelegramBot', { name }) && this.pairing?.name === name) { this.pairing = null; this.changed() }
  }
  async pair(name: string) {
    const reply = await this.act('pairTelegram', { name })
    const code = reply?.result?.code
    if (code) { this.pairing = { name, code }; this.changed() }
  }
  async setAllowed(name: string, userId: number, allowed: boolean) {
    await this.act('setTelegramAllowed', { name, userId, allowed })
  }

  /** Saves what the bot should know about a channel (a Telegram bot's name, or a Slack or Discord channel). */
  async setMemory(provider: 'slack' | 'discord' | 'telegram', channel: string, memory: string) {
    if (!this.active || this.busy) return false
    this.busy = true; this.error = null; this.changed()
    try {
      await (await this.client()).call('setChannelMemory', { provider, channel, memory })
      return true
    } catch (error) { if (this.active) this.error = String(error); return false }
    finally { if (this.active) { this.busy = false; this.changed() } }
  }
  async disconnectSlack() {
    if (!this.active || this.busy) return
    this.busy = true; this.error = null; this.changed()
    try {
      this.slack = await (await this.client()).call<SlackStatus>('disconnectSlack', {})
    } catch (error) { if (this.active) this.error = String(error) }
    finally { if (this.active) { this.busy = false; this.changed() } }
  }

  /** Saves the Discord connection; the host checks the token and every channel first. */
  async setDiscord(body: {
    guildId: string
    botUserId: string
    botToken: string
    bindings: { channelId: string; botId: string; allowedUsers: string[] }[]
  }) {
    if (!this.active || this.busy) return false
    this.busy = true; this.error = null; this.changed()
    try {
      this.discord = await (await this.client()).call<DiscordStatus>('setDiscordChannel', body)
      return true
    } catch (error) { if (this.active) this.error = String(error); return false }
    finally { if (this.active) { this.busy = false; this.changed() } }
  }

  async disconnectDiscord() {
    if (!this.active || this.busy) return
    this.busy = true; this.error = null; this.changed()
    try {
      this.discord = await (await this.client()).call<DiscordStatus>('disconnectDiscord', {})
    } catch (error) { if (this.active) this.error = String(error) }
    finally { if (this.active) { this.busy = false; this.changed() } }
  }
}
