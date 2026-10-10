import { useRef } from 'react'
import { isChat, type Entry } from '@shared/models'
import { groupExchanges, type BotExchange, type ChatSlot } from './bot-exchange'

export type ChatItem =
  | { id: string; kind: 'separator'; date: number }
  | { id: string; kind: 'entry'; entry: Entry; groupStart: boolean }
  | { id: string; kind: 'exchanges'; entry: Entry; exchanges: BotExchange[] }

/**
 * Chat-visible entries plus time separators (gaps > 1 h) and author grouping. A bot's messages
 * arrive whole (Grok Bot's `send_message`); what it writes along the way is trace. `grouped`
 * (the main transcript, not threads) folds runs of bot exchanges with one peer into one item.
 */
export function buildChat(entries: Entry[], grouped = false): ChatItem[] {
  const out: ChatItem[] = []
  let lastDate: number | null = null
  let lastAuthor: string | null = null
  const slots: ChatSlot[] = grouped ? groupExchanges(entries) : entries.filter(isChat).map((entry) => ({ entry, exchanges: null }))
  for (const { entry: e, exchanges } of slots) {
    const id = e.kind === 'user' && e.data.clientNonce ? `user-${e.data.clientNonce}` : e.id
    if (lastDate === null || e.createdAt - lastDate > 3_600_000) {
      out.push({ id: `sep-${id}`, kind: 'separator', date: e.createdAt })
      lastAuthor = null
    }
    if (exchanges) {
      out.push({ id, kind: 'exchanges', entry: e, exchanges })
      lastAuthor = null
      lastDate = e.createdAt
      continue
    }
    // In a group each bot is its own author.
    const author = e.kind === 'user' ? 'user' : e.kind === 'agent' ? `agent:${e.data.author ?? ''}` : e.kind
    out.push({ id, kind: 'entry', entry: e, groupStart: author !== lastAuthor })
    lastAuthor = author === 'user' || e.kind === 'agent' ? author : null
    lastDate = e.createdAt
  }
  return out
}

/**
 * Rows that arrived after the list first showed, while it follows the newest: they pop in
 * (`.row-in`, Grok Bot's). Earlier pages and rows that came in while reading history don't.
 */
export function useArrivals(ids: string[], following: boolean): Set<string> {
  const seen = useRef<Set<string> | null>(null)
  const fresh = useRef(new Set<string>())
  if (seen.current === null) seen.current = new Set(ids)
  for (const id of ids) {
    if (seen.current.has(id)) continue
    seen.current.add(id)
    if (following) fresh.current.add(id)
  }
  return fresh.current
}

/** A new row pops in from its bubble's side. */
export function rowIn(item: ChatItem): string {
  return item.kind === 'entry' && item.entry.kind === 'user' ? 'row-in mine' : 'row-in'
}

const time = new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' })
const monthDay = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' })
const weekday = new Intl.DateTimeFormat(undefined, { weekday: 'long' })

const sameDay = (a: Date, b: Date) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate()

export const relativeTime = {
  time: (ms: number) => time.format(new Date(ms)),
  /** Chat separator: Today 3:27 AM · Yesterday 5:20 PM · Sep 16 9:02 AM */
  separator(ms: number) {
    const date = new Date(ms)
    const now = new Date()
    const yesterday = new Date(now.getTime() - 86_400_000)
    const day = sameDay(date, now) ? 'Today' : sameDay(date, yesterday) ? 'Yesterday' : monthDay.format(date)
    return `${day} ${time.format(date)}`
  },
  /** Roster stamp: 3:45 AM · Yesterday · Wednesday · Sep 16 */
  day(ms: number) {
    const date = new Date(ms)
    const now = new Date()
    if (sameDay(date, now)) return time.format(date)
    if (sameDay(date, new Date(now.getTime() - 86_400_000))) return 'Yesterday'
    if (now.getTime() - ms < 6 * 86_400_000) return weekday.format(date)
    return monthDay.format(date)
  },
  /** Grok-style compact age: now · 5m · 3h · 2d · Sep 3 */
  short(ms: number) {
    const s = Math.max(0, (Date.now() - ms) / 1000)
    if (s < 60) return 'now'
    if (s < 3600) return `${Math.floor(s / 60)}m`
    if (s < 86_400) return `${Math.floor(s / 3600)}h`
    if (s < 86_400 * 7) return `${Math.floor(s / 86_400)}d`
    return monthDay.format(new Date(ms))
  },
}
