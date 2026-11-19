import { useCallback, useEffect, useRef, useState } from 'react'
import type { VoiceKey } from '@shared/voice-shortcut'
import type { Dictation } from './dictation'

/** A tap waits this long for a composer to take it (one may be opening). */
const KEY_WAIT_MS = 2000
/** The voice input shortcut's latest tap for the chat window's composer. */
let pendingKey: { key: VoiceKey; at: number } | null = null
const keyTakers = new Set<() => void>()

/** The chat window got a tap of the voice input shortcut (the island is off). */
export function queueVoiceKey(key: VoiceKey) {
  pendingKey = { key, at: Date.now() }
  for (const take of keyTakers) take()
}

/**
 * A submit for after dictation: it runs after the render that has the last words in the draft
 * (a submit right away would send the draft without them).
 */
export function useSubmitAfterRender(submit: () => void) {
  const [sending, setSending] = useState(false)
  useEffect(() => {
    if (!sending) return
    setSending(false)
    submit()
  }, [sending, submit])
  return useCallback(() => setSending(true), [])
}

/**
 * The voice input shortcut in the composer: a double tap dictates into it, the next tap sends
 * what was said once the last words are in (`send`, from `useSubmitAfterRender`), or only stops.
 */
export function useVoiceShortcut(dictation: Dictation, draft: string, send: () => void, focus: () => void) {
  const latest = useRef({ draft, focus })
  latest.current = { draft, focus }
  useEffect(() => {
    const take = () => {
      const pending = pendingKey
      pendingKey = null
      if (!pending || Date.now() - pending.at > KEY_WAIT_MS) return
      if (!dictation.shortcutKey(pending.key, send)) return
      latest.current.focus()
      dictation.start(latest.current.draft, true)
    }
    keyTakers.add(take)
    take()
    return () => void keyTakers.delete(take)
  }, [dictation, send])
}
