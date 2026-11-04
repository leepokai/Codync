import { useCallback, useEffect, useRef, useState } from 'react'
import { copyImage } from '../../lib/image-clipboard'
import { analytics } from '../../store/analytics'
import type { BotStore } from '../../store/bot-store'
import { useStoreRef } from '../../store/context'
import type { ImageSource } from './Attachments'

/** How long a copy's check mark (or failure) shows. */
const FLASH_MS = 1500

/** A picture's bytes from the computer, cached after the first read. */
export function imageBytes(store: BotStore, source: ImageSource) {
  return 'entryId' in source ? store.sharedFileData(source.file, source.entryId) : store.attachmentData(source.file.id, source.botId)
}

export type CopyResult = 'image' | 'name' | 'failed' | null

/** What can be done with a picture beyond looking at it: copy it, or copy its name, and which of those just happened. */
export function useImageActions(source: ImageSource) {
  const store = useStoreRef()
  const [done, setDone] = useState<CopyResult>(null)
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined)
  const flash = useCallback((result: CopyResult) => {
    setDone(result)
    clearTimeout(timer.current)
    timer.current = setTimeout(() => setDone(null), FLASH_MS)
  }, [])
  useEffect(() => () => clearTimeout(timer.current), [])

  return {
    done,
    /** The picture itself on the clipboard (not its name). */
    async copyImage() {
      try {
        const bytes = await imageBytes(store, source)
        if (!bytes) throw new Error('The picture is not loaded')
        await copyImage(bytes, source.file.name)
        analytics.track({ event: 'image_preview_action', action: 'copy_image' })
        flash('image')
      } catch (error) {
        console.error(error)
        flash('failed')
      }
    },
    copyName() {
      window.codync.app.copy(source.file.name)
      analytics.track({ event: 'image_preview_action', action: 'copy_name' })
      flash('name')
    },
  }
}
