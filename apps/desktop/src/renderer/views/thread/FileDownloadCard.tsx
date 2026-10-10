import type { Entry, SharedFile } from '@shared/models'
import { useStore } from '../../store/context'
import { Icon } from '../../components/Icon'
import { IconButtonBase } from '../../components/Overlay'
import { font } from '../../lib/fonts'
import { attachmentSymbol, byteCount } from './Attachments'

export function FileDownloadCard({ entry, file }: { entry: Entry; file: SharedFile }) {
  const store = useStore()
  const state = store.fileDownloads.states.get(file.id)
  const busy = state?.kind === 'downloading'
  const action = busy ? 'Cancel download' : state?.kind === 'failed' ? 'Retry download' : state?.kind === 'saved' ? 'Save again' : 'Download'
  const detail = busy ? `${byteCount(state.received)} of ${byteCount(file.size)}` : state?.kind === 'failed' ? state.message : state?.kind === 'saved' ? `${byteCount(file.size)} · Saved` : byteCount(file.size)
  return <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: 14, background: 'var(--bubble-agent)', borderRadius: 16, maxWidth: 380 }}>
    <Icon name={attachmentSymbol(file.name)} size={24} color="var(--secondary)" />
    <div style={{ minWidth: 0, flex: 1 }}>
      <div style={{ ...font('subheadline', 'semibold'), whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }} title={file.name}>{file.name}</div>
      <div role={state?.kind === 'failed' ? 'alert' : 'status'} style={{ ...font('caption2'), color: state?.kind === 'failed' ? 'var(--danger)' : 'var(--secondary)', overflowWrap: 'anywhere' }}>{detail}</div>
    </div>
    <IconButtonBase title={action} icon={busy ? 'xmark' : 'arrow.down.to.line'} size={36} onClick={() => busy ? store.fileDownloads.cancel(file.id) : void store.fileDownloads.save(store.client, entry.id, file)} />
  </div>
}
