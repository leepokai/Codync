import { useCallback, useEffect, useRef, useState } from 'react'
import type { Entry } from '@shared/models'
import type { MenuItem, ReactionPick } from '../../components/Menu'
import type { ImageFailure, ImageFrom } from '../../../shared/analytics.ts'
import { copies, copyImage, copyText } from '../../lib/image-clipboard'
import { copyFailure, imageActionEvent } from '../../lib/image-analytics.ts'
import { analytics } from '../../store/analytics'
import type { BotStore } from '../../store/bot-store'
import { useStoreRef } from '../../store/context'
import type { ImageSource } from './Attachments'
import { copiedPicture } from './message-copy'
import { trNow } from '../../lib/l10n.ts'

/** How long the copy button's check mark (or failure) shows. */
const FLASH_MS = 1500

/** A picture's bytes from the computer, cached after the first read. */
export function imageBytes(store: BotStore, source: ImageSource) {
  return 'entryId' in source ? store.sharedFileData(source.file, source.entryId) : store.attachmentData(source.file.id, source.botId)
}

/**
 * How a copy of the picture itself went. `overtaken`: a newer copy started before this one finished, so the clipboard and
 * showing how it went are that one's, and this one reports nothing.
 */
export type CopyOutcome = 'copied' | 'failed' | 'overtaken'

/** The picture itself on the clipboard (not its name). Reported, done or failed, unless a newer copy overtook it. */
export async function copyImageOf(store: BotStore, source: ImageSource, from: ImageFrom): Promise<CopyOutcome> {
  const request = copies.start()
  const current = () => copies.isLatest(request)
  let error: ImageFailure | undefined
  const bytes = await imageBytes(store, source).catch(console.error)
  if (!bytes) error = 'unavailable'
  else {
    try {
      await copyImage(bytes, current)
    } catch (cause) {
      console.error(cause)
      error = copyFailure(cause)
    }
  }
  if (!current()) return 'overtaken'
  analytics.track(imageActionEvent('copy_image', from, error))
  // The viewer's own copy button shows its result; a picture's menu elsewhere has nothing to show it.
  if (error && from === 'menu') store.lastError = trNow("Couldn't copy image")
  return error ? 'failed' : 'copied'
}

export function copyNameOf(source: ImageSource, from: ImageFrom) {
  void copyText(source.file.name, (name) => window.codync.app.copy(name))
  analytics.track(imageActionEvent('copy_name', from))
}

/** What replaces a message's plain Copy when it holds a picture: copy the picture, or just its name. `from` is where the menu is. */
export function imageCopyItems(store: BotStore, source: ImageSource, from: ImageFrom = 'menu', copy: () => void = () => void copyImageOf(store, source, from)): MenuItem[] {
  return [
    { title: 'Copy image', icon: 'photo.on.rectangle', action: copy },
    { title: 'Copy name', icon: 'square.on.square', action: () => copyNameOf(source, from) },
  ]
}

/** What a message's Copy is: its text, or for a picture, Copy image and Copy name. */
export function messageCopyItems(store: BotStore, entry: Entry): MenuItem[] {
  const picture = copiedPicture(entry)
  if (picture) return imageCopyItems(store, picture, 'menu')
  return [{ title: 'Copy', icon: 'square.on.square', action: () => entry.data.text && window.codync.app.copy(entry.data.text) }]
}

/** A message's own menu minus its Copy (Reply, what it did, …) and its reactions: a picture in it shows these with its own copy items. */
export interface MessageMenu {
  items: () => MenuItem[]
  reactions?: ReactionPick
}

export type CopyResult = 'image' | 'failed' | null

/** The viewer's copy button: copies the picture and which result to show for a moment. */
export function useImageCopy(source: ImageSource) {
  const store = useStoreRef()
  const [done, setDone] = useState<CopyResult>(null)
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined)
  useEffect(() => () => clearTimeout(timer.current), [])
  const copy = useCallback(async () => {
    const outcome = await copyImageOf(store, source, 'viewer')
    // A newer copy shows its own result.
    if (outcome === 'overtaken') return
    setDone(outcome === 'copied' ? 'image' : 'failed')
    clearTimeout(timer.current)
    timer.current = setTimeout(() => setDone(null), FLASH_MS)
  }, [store, source])
  return { done, copy }
}
