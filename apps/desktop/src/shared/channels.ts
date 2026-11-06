/** Messaging channels (Telegram, Slack) as the host reports them, and the wording the Channels screen uses. */

export interface TelegramBotStatus {
  name: string
  username: string
  botId: string
  botName: string
  allowed: number[]
  /** `ok`, `starting`, or `error: …`. */
  state: string
  lastReceivedAt: number | null
  pairingExpiresAt: number | null
}

export interface SlackStatus {
  configured: boolean
  socketMode: boolean
  workspaceId: string | null
  bindings: { channelId: string; botId: string; allowedUsers: string[] }[] | null
}

/** Same shape rules as the host's, so a mistake is told before the call. */
export const telegramNameOk = (name: string) => /^[a-z0-9-]{1,32}$/.test(name)
export const telegramTokenOk = (token: string) => token.length <= 128 && /^\d+:[A-Za-z0-9_-]{30,}$/.test(token)

export function telegramStateText(state: string): string {
  if (state === 'ok') return 'Connected'
  if (state === 'starting') return 'Starting…'
  return 'Cannot reach Telegram'
}

export function lastReceivedText(at: number | null, now: number): string {
  if (at === null) return 'Nothing received yet'
  const minutes = Math.max(0, Math.floor((now - at) / 60_000))
  if (minutes < 1) return 'Last message just now'
  if (minutes < 60) return `Last message ${minutes} min ago`
  if (minutes < 60 * 24) return `Last message ${Math.floor(minutes / 60)} h ago`
  return `Last message ${Math.floor(minutes / (60 * 24))} d ago`
}

/** What the person does with a pairing code. */
export function pairingInstruction(code: string, username: string): string {
  const where = username ? `@${username}` : 'your Telegram bot'
  return `Send /start ${code} to ${where} in Telegram. The code works once, for 10 minutes.`
}

export const allowedText = (count: number) => (count === 1 ? '1 person allowed' : `${count} people allowed`)
