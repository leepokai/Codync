import { useRef, type PointerEvent } from 'react'
import { isBelow } from '../../../shared/compat.ts'

/** The first host that takes a reply to a part of a message (`send`'s `replyPart`). */
const REPLY_PART_HOST = '2.48.1'

/** Whether the computer's host quotes a selected part; an older one drops it and quotes the whole message. */
export function canReplyToPart(hostVersion: string | undefined): boolean {
  return !!hostVersion && !isBelow(hostVersion, REPLY_PART_HOST)
}

/**
 * The text the user selected inside one message (`row`), which a reply then quotes in place of the whole message.
 * Empty when nothing is selected, or the selection reaches outside the message or sits in its own quote.
 */
export function selectedPartIn(row: Element): string {
  const selection = window.getSelection()
  if (!selection || selection.isCollapsed || !selection.rangeCount) return ''
  const within = selection.getRangeAt(0).commonAncestorContainer
  const element = within instanceof Element ? within : within.parentElement
  if (!element || !row.contains(element) || element.closest('.bubble-quote')) return ''
  return selection.toString().trim()
}

/**
 * What is selected in a message when a press starts on it (`keep`, on the row's pointer-down capture): by the time Reply
 * runs, the press may have changed the selection. `part` is what the last such press found.
 */
export function useSelectedPart() {
  const kept = useRef('')
  return {
    keep: (e: PointerEvent<HTMLElement>) => {
      // A press in one of the row's menus (drawn elsewhere on the page) keeps what the press that opened it found.
      if (e.currentTarget.contains(e.target as Node)) kept.current = selectedPartIn(e.currentTarget)
    },
    part: () => kept.current,
  }
}
