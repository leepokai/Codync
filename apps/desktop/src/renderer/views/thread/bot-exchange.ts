import { isChat, type Entry, type BotMessage } from '../../../shared/models.ts'

/** How a bot-to-bot message ended: still going, delivered, or it didn't complete. */
export type BotOutcome = { kind: 'pending' } | { kind: 'done' } | { kind: 'failed'; detail: string }

/** One bot-to-bot message as seen from `chat`'s own transcript. */
export interface BotExchange {
  id: string
  seq: number
  rev: number
  createdAt: number
  /** The bot whose chat this notice is in. */
  chat: string
  message: BotMessage
  outcome: BotOutcome
}

const SEPARATOR_GAP = 3_600_000

function outcomeOf(status: string | undefined, detail: string | undefined): BotOutcome {
  switch (status) {
    case 'completed':
      return { kind: 'done' }
    case 'failed':
    case 'cancelled':
      return { kind: 'failed', detail: detail ?? '' }
    default:
      return { kind: 'pending' }
  }
}

/** The structured exchange a notice carries, or null for any other entry. */
export function botExchange(e: Entry): BotExchange | null {
  const { botMessage: message, status } = e.data
  if (e.kind !== 'notice' || !message) return null
  if (typeof message.sourceBotId !== 'string' || typeof message.targetBotId !== 'string' || typeof message.text !== 'string') return null
  return { id: e.id, seq: e.seq, rev: e.rev, createdAt: e.createdAt, chat: e.botId, message, outcome: outcomeOf(status, message.detail) }
}

export const isOutgoing = (x: BotExchange) => x.chat === x.message.sourceBotId
export const peerOf = (x: BotExchange) => (isOutgoing(x) ? x.message.targetBotId : x.message.sourceBotId)
export const verbOf = (x: BotExchange) => (isOutgoing(x) ? 'Messaged' : 'Message from')

/** A chat-visible entry; a run of same-peer bot exchanges is one slot at its first entry. */
export interface ChatSlot {
  entry: Entry
  /** The run's exchanges (oldest first), or null for any other entry. */
  exchanges: BotExchange[] | null
}

/**
 * The chat-visible entries of a main transcript, with consecutive exchanges with the same peer
 * (either direction) merged. Anything else visible ends the run; trace entries don't.
 */
export function groupExchanges(entries: Entry[]): ChatSlot[] {
  const out: ChatSlot[] = []
  let run: ChatSlot | null = null
  for (const entry of entries) {
    if (!isChat(entry)) continue
    const x = botExchange(entry)
    if (!x) {
      out.push({ entry, exchanges: null })
      run = null
    } else if (run?.exchanges && peerOf(run.exchanges[0]!) === peerOf(x)) {
      run.exchanges.push(x)
    } else {
      run = { entry, exchanges: [x] }
      out.push(run)
    }
  }
  return out
}

/** What a group row shows: the peer, how many messages, and whether any failed. */
export function groupSummary(xs: BotExchange[]) {
  return { peerId: peerOf(xs[0]!), count: xs.length, failed: xs.some((x) => x.outcome.kind === 'failed') }
}

/**
 * The history between `chat` and `peer`: fetched entries plus the live chat, by id (the higher
 * rev wins), oldest first.
 */
export function botConversation(fetched: Entry[], live: Entry[], peer: string): BotExchange[] {
  const byId = new Map<string, BotExchange>()
  for (const e of [...fetched, ...live]) {
    const x = botExchange(e)
    if (!x) continue
    const known = byId.get(x.id)
    if (!known || x.rev >= known.rev) byId.set(x.id, x)
  }
  return [...byId.values()].filter((x) => peerOf(x) === peer).sort((a, b) => a.seq - b.seq)
}

export type BotConversationRow =
  | { kind: 'separator'; id: string; at: number }
  | { kind: 'message'; id: string; author: string; text: string; showsAuthor: boolean }
  | { kind: 'status'; id: string; outcome: BotOutcome; awaiting: string }

/** The sheet's rows: time separators, request and reply messages, and a caption while pending or failed. */
export function buildBotConversation(xs: BotExchange[]): BotConversationRow[] {
  const rows: BotConversationRow[] = []
  let previousAt: number | null = null
  let author: string | null = null
  const message = (id: string, by: string, text: string) => {
    rows.push({ kind: 'message', id, author: by, text, showsAuthor: by !== author })
    author = by
  }
  for (const x of xs) {
    if (previousAt === null || x.createdAt - previousAt > SEPARATOR_GAP) {
      rows.push({ kind: 'separator', id: `sep-${x.id}`, at: x.createdAt })
      author = null
    }
    previousAt = x.createdAt
    message(`${x.id}-request`, x.message.sourceBotId, x.message.text)
    if (x.message.reply) message(`${x.id}-reply`, x.message.targetBotId, x.message.reply)
    if (x.outcome.kind !== 'done') {
      rows.push({ kind: 'status', id: `${x.id}-status`, outcome: x.outcome, awaiting: x.message.targetBotId })
      author = null
    }
  }
  return rows
}
