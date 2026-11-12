import { useEffect, useRef, useState, type ReactNode } from 'react'
import { isAudioName, isImageName, type Attachment, type SharedFile } from '@shared/models'
import { Icon } from '../../components/Icon'
import { IconButtonBase, ModalHeader, Sheet, SheetPage } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { imageActionEvent } from '../../lib/image-analytics.ts'
import { analytics } from '../../store/analytics'
import { useStoreRef } from '../../store/context'
import { byteCount } from './file-card'
import { ImageContextMenu } from './ImageContextMenu'
import { leavingViewer } from './message-copy'
import { imageBytes, useImageCopy, type MessageMenu } from './use-image-actions'
import { ZoomableImage } from './ZoomableImage'
import './attachments.css'

/** The symbol for a file by its extension. */
export function attachmentSymbol(name: string) {
  const ext = name.split('.').pop()?.toLowerCase() ?? ''
  if (isImageName(name)) return 'photo'
  if (ext === 'pdf') return 'doc.richtext'
  if (['zip', 'gz', 'tar', '7z'].includes(ext)) return 'doc.zipper'
  if (['mov', 'mp4', 'm4v'].includes(ext)) return 'film'
  if (isAudioName(name)) return 'waveform'
  return 'doc'
}

/** Files sent with a message: images as pictures (click for full size), other files as cards. */
export function AttachmentList({ attachments, botId, menu }: { attachments: Attachment[]; botId: string; menu?: MessageMenu }) {
  const [viewing, setViewing] = useState<Attachment | null>(null)
  return (
    <VStack align="flex-end" gap={4}>
      {attachments.map((file) =>
        isImageName(file.name) ? (
          <ImageContextMenu key={file.id} source={{ file, botId }} menu={menu}>
            <button className="attachment-image-button" aria-label={file.name} onClick={() => setViewing(file)}>
              <AttachmentImage source={{ file, botId }} fit="thumbnail" />
            </button>
          </ImageContextMenu>
        ) : (
          <HStack key={file.id} gap={8} className="attachment-file">
            <Icon name={attachmentSymbol(file.name)} size={16} color="var(--secondary)" />
            <VStack gap={1} shrink>
              <Text size="subheadline" tone="text" truncate className="attachment-file-name">{file.name}</Text>
              <Text size="caption2" tone="tertiary">{byteCount(file.size)}</Text>
            </VStack>
          </HStack>
        ),
      )}
      <AttachmentViewer source={viewing && { file: viewing, botId }} close={() => setViewing(null)} menu={menu} />
    </VStack>
  )
}

/** Where a picture's bytes are on the computer: a file sent with a message, or one a bot shared. */
export type ImageSource = { file: Attachment; botId: string } | { file: SharedFile; entryId: string }

/** A picture full size, in a sheet: zoom it, copy it, and with `actions` (a shared file's download) next to the close button. */
export function AttachmentViewer({ source, close, actions, note, menu }: { source: ImageSource | null; close: () => void; actions?: ReactNode; note?: ReactNode; menu?: MessageMenu }) {
  const id = source?.file.id
  // Each opening is a new page, even one that starts while the sheet is still closing (which keeps its last content).
  const shown = useRef<string>(undefined)
  const opening = useRef(0)
  if (id && id !== shown.current) opening.current++
  shown.current = id
  useEffect(() => {
    if (id) analytics.track({ event: 'image_preview_opened' })
  }, [id])
  return (
    <Sheet open={source !== null} onClose={close} width={1040} height={800}>
      {source ? <ViewerPage key={opening.current} source={source} actions={actions} note={note} menu={menu && leavingViewer(menu, close)} /> : null}
    </Sheet>
  )
}

function ViewerPage({ source, actions, note, menu }: { source: ImageSource; actions?: ReactNode; note?: ReactNode; menu?: MessageMenu }) {
  const copy = useImageCopy(source)
  // Zooming is counted once per opening, not per pinch step.
  const zoomed = useRef(false)
  const reportZoom = () => {
    if (zoomed.current) return
    zoomed.current = true
    analytics.track(imageActionEvent('zoom', 'viewer'))
  }
  const [title, icon] = copy.done === 'image' ? ['Copied', 'checkmark'] : copy.done === 'failed' ? ["Couldn't copy image", 'exclamationmark.circle'] : ['Copy image', 'doc.on.doc']
  return (
    <SheetPage>
      <ModalHeader
        title={source.file.name}
        trailing={
          <>
            <IconButtonBase title={title} icon={icon} onClick={() => void copy.copy()} />
            {actions}
          </>
        }
      />
      {note}
      <ImageContextMenu source={source} menu={menu} copyImage={() => void copy.copy()} className="attachment-viewer">
        <ZoomableImage onZoom={reportZoom}>
          <AttachmentImage source={source} fit="full" />
        </ZoomableImage>
      </ImageContextMenu>
    </SheetPage>
  )
}

/** A picture from the computer, fetched once and then read from the cache. */
export function AttachmentImage({ source, fit }: { source: ImageSource; fit: 'icon' | 'thumbnail' | 'full' }) {
  const store = useStoreRef()
  const [url, setUrl] = useState<string | null>(null)
  useEffect(() => {
    let revoked = false
    let made: string | null = null
    const read = () => {
      void imageBytes(store, source).then((data) => {
        if (!data || revoked || made) return
        made = URL.createObjectURL(new Blob([data as BlobPart]))
        setUrl(made)
      })
    }
    // A chat shown before the computer connects reads its pictures again once it comes online.
    let online = store.connection.kind === 'online'
    const stop = store.subscribe(() => {
      const now = store.connection.kind === 'online'
      if (now && !online && !made) read()
      online = now
    })
    read()
    return () => {
      revoked = true
      stop()
      if (made) URL.revokeObjectURL(made)
    }
    // A file's id names its bytes for good; `source` is a new object every render.
  }, [store, source.file.id])
  if (!url) {
    return (
      <div className={`attachment-placeholder ${fit}`}>
        <Icon name="photo" size={13} color="var(--tertiary)" />
      </div>
    )
  }
  return <img src={url} alt="" draggable={false} className={`attachment-image ${fit}`} />
}
