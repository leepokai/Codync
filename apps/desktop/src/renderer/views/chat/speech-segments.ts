import type { SpeechEvent } from '../../../shared/ipc.ts'
import { joinDictated } from './dictation-pieces.ts'

/** A result this long after the one before follows a pause; revisions come while the words are still being said. */
const PAUSE_MS = 1000

/**
 * The on-device recognizer's results as one transcript. After a pause of a second or two,
 * on-device SFSpeechRecognizer (macOS 15+, iOS 18+) starts its transcript over with only the new
 * words and no final result; the words before stay, and the new ones go after them.
 */
export class SpeechSegments {
  /** The segments before the current one. */
  private done = ''
  private current = ''
  /** The recognizer closed `current` (a final result, or one marking the end of what was said). */
  private ended = false
  /** When `current` came. */
  private last = 0

  /** Everything heard so far, given the recognizer's latest result. */
  heard(e: Extract<SpeechEvent, { type: 'partial' | 'final' }>, at = Date.now()): string {
    const ends = e.type === 'final' || e.ends === true
    if (this.current && this.startsOver(e.text, ends, at - this.last)) this.done = joinDictated(this.done, this.current)
    this.current = e.text
    this.ended = ends
    this.last = at
    return joinDictated(this.done, e.text)
  }

  private startsOver(text: string, ends: boolean, gap: number) {
    // After a closed segment, only all of its words going on are the same segment; other words are the next one.
    if (this.ended) return !continues(this.current, text, 1)
    if (continues(this.current, text, 0.5)) return false
    // An empty result keeps the words before it.
    if (!text) return true
    // A result that closes the segment is its last revision.
    if (ends) return false
    // ponytail: no end marker, so other words after a pause are a new segment; a revision of more than half
    // the segment's start that comes after a pause stays beside the words it revises.
    return gap >= PAUSE_MS
  }
}

/** `text` starts with at least `share` of `current`, ignoring case, spaces and punctuation: the same words, revised or going on. */
function continues(current: string, text: string, share: number) {
  const [a, b] = [letters(current), letters(text)]
  let same = 0
  while (same < a.length && a[same] === b[same]) same++
  return same >= a.length * share
}

const letters = (text: string) => [...text.toLowerCase().replace(/[^\p{L}\p{N}]/gu, '')]
