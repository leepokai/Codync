import { useState } from 'react'
import { showsAsImage, type Entry, type SharedFile } from '@shared/models'
import { useStore } from '../../store/context'
import type { DownloadState } from '../../store/file-downloads'
import { Icon } from '../../components/Icon'
import { IconButtonBase } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { analytics } from '../../store/analytics'
import { AttachmentImage, AttachmentViewer, attachmentSymbol, byteCount, type ImageSource } from './Attachments'
import { ImageContextMenu } from './ImageContextMenu'
import type { MessageMenu } from './use-image-actions'
import { trNow } from '../../lib/l10n.ts'
import './attachments.css'

/** What the download button does now (also the Library's file rows). */
export function downloadAction(state: DownloadState | undefined) {
  return state?.kind === 'downloading' ? trNow('Cancel download') : state?.kind === 'failed' ? trNow('Retry download') : state?.kind === 'saved' ? trNow('Save again') : trNow('Download')
}

/** The line under a shared file's name: its size, the download's progress, or what went wrong. */
export function downloadDetail(state: DownloadState | undefined, file: SharedFile) {
  switch (state?.kind) {
    case 'downloading': return `${byteCount(state.received)} of ${byteCount(file.size)}`
    case 'failed': return state.message
    case 'saved': return `${byteCount(file.size)} · Saved`
    default: return byteCount(file.size)
  }
}

/** Saves a shared file (or stops that); the same button on its card and beside the close button of its full-size picture. */
function DownloadButton({ entry, file, inViewer = false }: { entry: Entry; file: SharedFile; inViewer?: boolean }) {
  const store = useStore()
  const state = store.fileDownloads.states.get(file.id)
  const busy = state?.kind === 'downloading'
  const start = () => {
    if (busy) return store.fileDownloads.cancel(file.id)
    if (inViewer) analytics.track({ event: 'image_preview_action', action: 'download' })
    void store.fileDownloads.save(store.client, entry.id, file)
  }
  return <IconButtonBase title={downloadAction(state)} icon={busy ? 'xmark' : 'arrow.down.to.line'} size={inViewer ? 28 : 36} onClick={start} />
}

export function FileDownloadCard({ entry, file, menu }: { entry: Entry; file: SharedFile; menu?: MessageMenu }) {
  const store = useStore()
  const state = store.fileDownloads.states.get(file.id)
  const detail = downloadDetail(state, file)
  const [viewing, setViewing] = useState(false)
  const image: ImageSource = { file, entryId: entry.id }
  return <VStack gap={12} className="file-card">
    {showsAsImage(file) ? <>
      <ImageContextMenu source={image} menu={menu}>
        <button className="file-card-image" aria-label={file.name} onClick={() => setViewing(true)}>
          <AttachmentImage source={image} fit="thumbnail" />
        </button>
      </ImageContextMenu>
      <AttachmentViewer source={viewing ? image : null} close={() => setViewing(false)} menu={menu} actions={<DownloadButton entry={entry} file={file} inViewer />} />
    </> : null}
    <HStack gap={12}>
      <Icon name={attachmentSymbol(file.name)} size={24} color="var(--secondary)" />
      <VStack grow>
        <Text as="div" size="subheadline" weight="semibold" truncate title={file.name}>{file.name}</Text>
        <Text as="div" size="caption2" tone={state?.kind === 'failed' ? 'danger' : 'secondary'} role={state?.kind === 'failed' ? 'alert' : 'status'} className="file-card-detail">{detail}</Text>
      </VStack>
      <DownloadButton entry={entry} file={file} />
    </HStack>
  </VStack>
}
