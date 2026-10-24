// Voice calls in a chat (host/src/chat/calls): what a call sends the bot, and the bot's replies while it lasts,
// carry its id; the chat shows them only in the call's conversation, which its line opens.

import type { Entry } from './models'

/** One line of a voice call: what the user said (`user`) or the voice answered (`voice`), with when (ms). */
export interface CallLine {
  from: 'user' | 'voice'
  text: string
  at?: number
}

/** One message of a call's conversation. */
export interface CallMessage {
  speaker: 'user' | 'voice' | 'voiceToBot' | 'bot'
  text: string
}

/** Without what voice calls carried: the messages of a call that has its line, or of a call this window is on. One
 * that failed to send stays, to be sent again. */
export function hidingCalls(entries: Entry[], live: ReadonlySet<string>): Entry[] {
  const ended = new Set(entries.flatMap((e) => (e.kind === 'notice' && e.data.call ? [e.data.call] : [])))
  const shown = (e: Entry) => e.kind === 'notice' || !e.data.call || e.data.status === 'failed'
  return entries.filter((e) => shown(e) || (!ended.has(e.data.call!) && !live.has(e.data.call!)))
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
