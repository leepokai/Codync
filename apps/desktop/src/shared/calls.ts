// Voice calls in a chat (host/src/chat/calls): what a call sends the bot, and the bot's replies while it lasts,
// carry its id; the chat shows them only in the call's conversation, which its line opens.

import type { Entry } from './models'

/** One line of a voice call: what the user said (`user`) or the voice answered (`voice`), with when (ms). */
export interface CallLine {
  from: 'user' | 'voice'
  text: string
  at?: number
  /** The app's id for the line: one sent again (its first answer lost) is kept once. */
  id?: string
}

/** Something the computer tells a call's voice while the call lasts: the bot's reply, a notice (an approval waiting,
 * a failure) or what the bot is doing. Ready to say: worded for the call's voice, in a group with who said it. */
export interface CallCue {
  call: string
  kind: 'reply' | 'notice' | 'progress'
  text: string
  /** Rises with each cue of the call: one heard already (replayed after a reconnect) is skipped. */
  seq: number
}

const CUE_KINDS: ReadonlySet<string> = new Set(['reply', 'notice', 'progress'])

/** A `call` event, or null for one this app can't use (a newer host's new kind is passed over). */
export function parseCallCue(raw: Record<string, unknown>): CallCue | null {
  const { call, kind, text, seq } = raw
  return typeof call === 'string' && typeof kind === 'string' && CUE_KINDS.has(kind) && typeof text === 'string' && typeof seq === 'number'
    ? { call, kind: kind as CallCue['kind'], text, seq }
    : null
}

/** One message of a call's conversation. */
export interface CallMessage {
  speaker: 'user' | 'voice' | 'voiceToBot' | 'bot'
  text: string
}

/**
 * Without what voice calls carry, whichever device the call is on: it shows behind the call's line, which the host
 * writes when the call ends or its app went away. A message that failed to send stays, to be sent again, and a file or
 * an artifact the bot sent, to be opened.
 */
export function hidingCalls(entries: Entry[]): Entry[] {
  const shown = (e: Entry) => e.kind === 'notice' || !e.data.call || e.data.status === 'failed'
  return entries.filter((e) => shown(e) || !!e.data.files?.length || !!e.data.artifact)
}

/** A call's conversation, oldest first: what was said (the voice's transcript) and the messages it carried. */
export function callConversation(entries: Entry[], line: Entry): CallMessage[] {
  const call = line.data.call
  if (!call) return []
  const said: { at: number; message: CallMessage }[] = (line.data.transcript ?? []).map((l) => ({
    at: l.at ?? line.createdAt,
    message: { speaker: l.from === 'user' ? 'user' : 'voice', text: l.text },
  }))
  for (const e of entries) {
    if (e.data.call !== call || !(e.kind === 'user' || (e.kind === 'agent' && e.data.final))) continue
    const speaker = e.kind === 'agent' ? 'bot' : e.data.from === 'voice' ? 'voiceToBot' : 'user'
    said.push({ at: e.createdAt, message: { speaker, text: e.data.text ?? '' } })
  }
  // Stable: phone and computer clocks are merged as they are.
  return said.map((s, i) => ({ ...s, i })).sort((a, b) => a.at - b.at || a.i - b.i).map((s) => s.message)
}

/** The chat's entries with a call's own (`callEntries`), which the chat may not hold: each entry once, the chat's copy first. */
export function withCallEntries(entries: Entry[], fetched: Entry[]): Entry[] {
  const held = new Set(entries.map((e) => e.id))
  return [...entries, ...fetched.filter((e) => !held.has(e.id))]
}
