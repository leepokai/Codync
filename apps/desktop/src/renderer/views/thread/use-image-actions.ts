import { useCallback, useEffect, useRef, useState } from 'react'
import type { Entry } from '@shared/models'
import type { MenuItem, ReactionPick } from '../../components/Overlay'
import { copyImage } from '../../lib/image-clipboard'
import { analytics } from '../../store/analytics'
import type { BotStore } from '../../store/bot-store'
import { useStoreRef } from '../../store/context'
import type { ImageSource } from './Attachments'
import { copiedPicture } from './message-copy'

/** How long a copy's check mark (or failure) shows. */
const FLASH_MS = 1500

/** A picture's bytes from the computer, cached after the first read. */
export function imageBytes(store: BotStore, source: ImageSource) {
  return 'entryId' in source ? store.sharedFileData(source.file, source.entryId) : store.attachmentData(source.file.id, source.botId)
}

/** The picture itself on the clipboard (not its name). Whether it worked. */
export async function copyImageOf(store: BotStore, source: ImageSource) {
  try {
    const bytes = await imageBytes(store, source)
    if (!bytes) throw new Error('The picture is not loaded')
    await copyImage(bytes, source.file.name)
    analytics.track({ event: 'image_preview_action', action: 'copy_image' })
    return true
  } catch (error) {
    console.error(error)
    return false
  }
}

export function copyNameOf(source: ImageSource) {
  window.codync.app.copy(source.file.name)
  analytics.track({ event: 'image_preview_action', action: 'copy_name' })
}

/** What replaces a message's plain Copy when it holds a picture: copy the picture, or just its name. */
export function imageCopyItems(store: BotStore, source: ImageSource, copy: () => void = () => void copyImageOf(store, source)): MenuItem[] {
  return [
    { title: 'Copy image', icon: 'photo.on.rectangle', action: copy },
    { title: 'Copy name', icon: 'square.on.square', action: () => copyNameOf(source) },
  ]
}

/** What a message's Copy is: its text, or for a picture, Copy image and Copy name. */
export function messageCopyItems(store: BotStore, entry: Entry): MenuItem[] {
  const picture = copiedPicture(entry)
  if (picture) return imageCopyItems(store, picture)
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
    setDone((await copyImageOf(store, source)) ? 'image' : 'failed')
    clearTimeout(timer.current)
    timer.current = setTimeout(() => setDone(null), FLASH_MS)
  }, [store, source])
  return { done, copy }
}
