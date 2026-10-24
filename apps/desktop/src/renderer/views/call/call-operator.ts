import { isChat, isGroup, type Entry } from '@shared/models'
import type { BotStore } from '../../store/bot-store'
import { spokenText } from './voice'

export interface OperatorTool {
  name: string
  description: string
  /** JSON Schema (both providers take the same shape). */
  parameters: { type: 'object'; properties: Record<string, { type: string; description: string }>; required?: string[] }
}

export const OPERATOR_TOOLS: OperatorTool[] = [
  {
    name: 'send_to_bot',
    description: 'Send a message to the bot on the computer, exactly as if the user typed it. Its reply arrives later.',
    parameters: { type: 'object', properties: { text: { type: 'string', description: 'The message' } }, required: ['text'] },
  },
  {
    name: 'bot_status',
    description: 'Whether the bot is idle or working, what it is doing, and any approval it is waiting for.',
    parameters: { type: 'object', properties: {} },
  },
  {
    name: 'recent_messages',
    description: 'The latest messages in the chat with the bot, oldest first.',
    parameters: { type: 'object', properties: { count: { type: 'integer', description: 'How many, 1–20' } } },
  },
  {
    name: 'answer_approval',
    description: "Answer the bot's pending approval request with one of its options.",
    parameters: { type: 'object', properties: { option: { type: 'string', description: "The option's name" } }, required: ['option'] },
  },
]

/**
 * A realtime model is an operator in front of the bot, not a replacement for it: the bot does the
 * work, the operator talks. Its tools run here against the store; only `send_to_bot` messages and
 * the bot's replies become chat entries (kit CallOperator).
 */
export class CallOperator {
  constructor(
    private store: BotStore,
    private botId: string,
  ) {}

  get instructions() {
    const bot = this.store.bots.get(this.botId)
    const name = bot?.name ?? 'the bot'
    const about = bot?.description ? ` (${bot.description})` : ''
    const who = bot && isGroup(bot)
      ? `the group chat ${name}${about} on the user's computer, where the bots ${this.store.members(bot).map((m) => m.name).join(', ')} answer in turn (a message that @-mentions one of them asks just that bot). The bots do all the actual work; you relay`
      : `${name}${about}, a coding agent running on the user's computer. ${name} does all the actual work; you relay`
    return [
      `You are the voice on a phone call between the user and ${who}.`,
      '- When the user asks for something to be done, changed, checked or answered about their code or computer, call send_to_bot with their request in their own words (keep details, names and code terms exactly). Don\'t do the work or invent results yourself.',
      '- Use bot_status and recent_messages for "what are you doing?" or "what did it say?".',
      '- Call answer_approval only after the user has clearly said which option to pick.',
      `- Messages starting with [${name} replied] or [Notice] come from the computer: tell the user what they say, shortened for speech (no code blocks, no long lists, no URLs).`,
      '- Reply in the user\'s language. Keep spoken answers short and natural.',
    ].join('\n')
  }

  /** Runs one tool call; the result is a JSON string. */
  call(name: string, args: Record<string, unknown>): string {
    let result: Record<string, unknown>
    switch (name) {
      case 'send_to_bot':
        result = this.send(typeof args.text === 'string' ? args.text : '')
        break
      case 'bot_status':
        result = this.status()
        break
      case 'recent_messages':
        result = this.recent(typeof args.count === 'number' ? Math.trunc(args.count) : 6)
        break
      case 'answer_approval':
        result = this.answer(typeof args.option === 'string' ? args.option : '')
        break
      default:
        result = { error: `Unknown tool ${name}.` }
    }
    return JSON.stringify(result)
  }

  /** How a reply is handed to the operator. */
  reply(text: string) {
    return `[${this.store.bots.get(this.botId)?.name ?? 'Bot'} replied] ${spokenText(text)}`
  }

  notice(text: string) {
    return `[Notice] ${text}`
  }

  private send(text: string) {
    const trimmed = text.trim()
    if (!trimmed) return { error: 'Nothing to send.' }
    this.store.send(trimmed, this.botId)
    return { sent: true, note: "The bot's reply will arrive as a separate message." }
  }

  private status() {
    const bot = this.store.bots.get(this.botId)
    if (!bot) return { error: "The bot isn't available." }
    const out: Record<string, unknown> = { status: bot.status, activity: bot.activity }
    const card = this.pendingApproval
    if (card) out.approval = { request: card.data.title ?? card.data.command ?? '', options: (card.data.options ?? []).map((o) => o.name) }
    return out
  }

  private recent(count: number) {
    const name = this.store.bots.get(this.botId)?.name ?? 'bot'
    const n = Math.min(Math.max(count, 1), 20)
    const messages = this.store
      .allEntries(this.botId)
      .filter(isChat)
      .slice(-n)
      .map((e) => ({
        from: e.kind === 'user' ? 'user' : e.kind === 'agent' ? (e.data.author && this.store.bots.get(e.data.author)?.name) || name : e.kind === 'permission' ? 'approval request' : 'notice',
        text: spokenText(e.data.text ?? e.data.title ?? ''),
      }))
    return { messages }
  }

  private answer(option: string) {
    const card = this.pendingApproval
    if (!card) return { error: 'Nothing is waiting for approval.' }
    const options = card.data.options ?? []
    const wanted = option.toLowerCase()
    const pick =
      options.find((o) => o.name.toLowerCase() === wanted || o.optionId === option) ??
      options.find((o) => o.name.toLowerCase().includes(wanted) || wanted.includes(o.name.toLowerCase()))
    if (!pick) return { error: 'No such option.', options: options.map((o) => o.name) }
    this.store.respond(card, pick.optionId)
    return { answered: pick.name }
  }

  private get pendingApproval(): Entry | undefined {
    return this.store.allEntries(this.botId).findLast((e) => e.kind === 'permission' && e.data.status === 'pending')
  }
}
