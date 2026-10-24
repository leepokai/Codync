import type { Entry, SharedFile } from '@shared/models'
import { useStore } from '../../store/context'
import { Icon } from '../../components/Icon'
import { IconButtonBase } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { attachmentSymbol, byteCount } from './Attachments'
import './attachments.css'

export function FileDownloadCard({ entry, file }: { entry: Entry; file: SharedFile }) {
  const store = useStore()
  const state = store.fileDownloads.states.get(file.id)
  const busy = state?.kind === 'downloading'
  const action = busy ? 'Cancel download' : state?.kind === 'failed' ? 'Retry download' : state?.kind === 'saved' ? 'Save again' : 'Download'
  const detail = busy ? `${byteCount(state.received)} of ${byteCount(file.size)}` : state?.kind === 'failed' ? state.message : state?.kind === 'saved' ? `${byteCount(file.size)} · Saved` : byteCount(file.size)
  return <HStack gap={12} className="file-card">
    <Icon name={attachmentSymbol(file.name)} size={24} color="var(--secondary)" />
    <VStack grow>
      <Text as="div" size="subheadline" weight="semibold" truncate title={file.name}>{file.name}</Text>
      <Text as="div" size="caption2" tone={state?.kind === 'failed' ? 'danger' : 'secondary'} role={state?.kind === 'failed' ? 'alert' : 'status'} className="file-card-detail">{detail}</Text>
    </VStack>
    <IconButtonBase title={action} icon={busy ? 'xmark' : 'arrow.down.to.line'} size={36} onClick={() => busy ? store.fileDownloads.cancel(file.id) : void store.fileDownloads.save(store.client, entry.id, file)} />
  </HStack>
}
