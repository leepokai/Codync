import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react'
import type { Entry } from '@shared/models'
import type { MenuItem, ReactionPick } from '../../components/Overlay'
import type { ImageFailure, ImageFrom } from '../../../shared/analytics.ts'
import { copyImage } from '../../lib/image-clipboard'
import { copyFailure, imageActionEvent } from '../../lib/image-analytics.ts'
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

/** Whether a copy from a picture's menu just failed: nothing on screen shows its result, so the thread says so for a moment. */
let copyFailed = false
const failureListeners = new Set<() => void>()
let failureTimer: ReturnType<typeof setTimeout> | undefined
function flashCopyFailure() {
  copyFailed = true
  failureListeners.forEach((listener) => listener())
  clearTimeout(failureTimer)
  failureTimer = setTimeout(() => {
    copyFailed = false
    failureListeners.forEach((listener) => listener())
  }, FLASH_MS * 2)
}
export function useCopyFailed() {
  return useSyncExternalStore((listener) => { failureListeners.add(listener); return () => void failureListeners.delete(listener) }, () => copyFailed)
}

/** The picture itself on the clipboard (not its name). Whether it worked; reported either way. */
export async function copyImageOf(store: BotStore, source: ImageSource, from: ImageFrom) {
  let error: ImageFailure | undefined
  const bytes = await imageBytes(store, source).catch(console.error)
  if (!bytes) error = 'unavailable'
  else {
    try {
      await copyImage(bytes, source.file.name)
    } catch (cause) {
      console.error(cause)
      error = copyFailure(cause)
    }
  }
  analytics.track(imageActionEvent('copy_image', from, error))
  // The viewer's own copy button shows its result; a picture's menu elsewhere has nothing to show it.
  if (error && from === 'menu') flashCopyFailure()
  return !error
}

export function copyNameOf(source: ImageSource, from: ImageFrom) {
  window.codync.app.copy(source.file.name)
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
    setDone((await copyImageOf(store, source, 'viewer')) ? 'image' : 'failed')
    clearTimeout(timer.current)
    timer.current = setTimeout(() => setDone(null), FLASH_MS)
  }, [store, source])
  return { done, copy }
}
