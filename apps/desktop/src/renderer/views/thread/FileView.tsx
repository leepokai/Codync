import { showsAsImage, type Entry, type SharedFile } from '@shared/models'
import { Button, IconButton } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { AttachmentImage, attachmentSymbol, byteCount } from './Attachments'
import { downloadAction, downloadDetail } from './FileDownloadCard'

/** A shared file full size over the chat: its picture, or its name and size, and the download action. */
export function FileView({ entryId, file, close }: { entryId: string; file: SharedFile; close: () => void }) {
  const tr = useTr()
  const store = useStore()
  const state = store.fileDownloads.states.get(file.id)
  const busy = state?.kind === 'downloading'
  return (
    <div className="artifact-layer fade-in">
      <div className="top-chrome">
        <div className="top-chrome-row">
          <IconButton title={tr('Close file')} icon="xmark" onClick={close} />
        </div>
      </div>
      <VStack gap={12} className="file-view">
        {showsAsImage(file) ? (
          <div className="file-view-image"><AttachmentImage source={{ file, entryId }} fit="full" /></div>
        ) : (
          <HStack gap={12}>
            <IconButton title={tr(downloadAction(state))} icon={busy ? 'xmark' : 'arrow.down.to.line'} onClick={() => busy ? store.fileDownloads.cancel(file.id) : void store.fileDownloads.save(store.client, entryId, file)} />
            <Text weight="semibold" truncate>{file.name}</Text>
          </HStack>
        )}
        <HStack gap={8} align="center">
          <Text size="caption" tone={state?.kind === 'failed' ? 'danger' : 'secondary'}>{tr(downloadDetail(state, file))}</Text>
          <Button kind="secondary" onClick={() => busy ? store.fileDownloads.cancel(file.id) : void store.fileDownloads.save(store.client, entryId, file)}>
            {tr(downloadAction(state))}
          </Button>
          <IconButton title={file.name} icon={attachmentSymbol(file.name)} onClick={() => void 0} />
          <Text size="caption" tone="tertiary">{byteCount(file.size)}</Text>
        </HStack>
      </VStack>
    </div>
  )
}

export type OpenFile = { entryId: string; file: SharedFile; entry: Entry }
