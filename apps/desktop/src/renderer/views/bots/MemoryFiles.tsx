import { useEffect, useState, type ReactElement } from 'react'
import { CardForm, CardSection } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader } from '../../components/Overlay'
import type { MemoryFile, MemoryFileDetail, MemoryFiles, MemoryStore } from '../../store/memory-store'
import { relativeTime } from '../thread/chat-items'

/** The note a sheet opens. */
export interface FileTarget { name: string; shared: boolean }

/** One layer's long-form notes, as rows under its entries: name, description, size and date, marked past a cap. Read-only: the bot writes them. */
export function fileRows(files: MemoryFiles, shared: boolean, open: (target: FileTarget) => void, tr: (text: string) => string = (text) => text): ReactElement[] {
  const live = (shared ? files.yours : files.bot).files.filter((f) => !f.deleted)
  return [
    <p key="notes" className="memory-notes-header">{tr('Notes')}</p>,
    ...(live.length === 0 ? [<p key="notes-empty" className="memory-muted">{tr('Longer notes, one topic each. The bot reads one when its topic comes up.')}</p>] : []),
    ...(live.length > files.folderCap ? [<p key="notes-cap" className="memory-attention">{live.length} notes, over the {files.folderCap}-note cap. Ask the bot to merge some.</p>] : []),
    ...live.map((file) => <FileRow key={file.name} file={file} cap={files.cap} tr={tr} open={() => open({ name: file.name, shared })} />),
  ]
}

function FileRow({ file, cap, tr, open }: { file: MemoryFile; cap: number; tr: (text: string) => string; open: () => void }) {
  const over = file.tokens > cap
  return <button className="press memory-content fade-in" onClick={open}>
    <strong><Icon name="doc.text" size={11} />{file.name}</strong>
    {file.description ? <span className="memory-muted">{file.description}</span> : null}
    <small className={over ? 'memory-attention' : undefined}>
      {file.tokens} tokens{over ? `, over the ${cap} cap` : ''} · {relativeTime.day(file.updatedAt, tr)}
    </small>
  </button>
}

/** One note, to read. */
export function MemoryFileSheet({ memory, target }: { memory: MemoryStore; target: FileTarget }) {
  const [detail, setDetail] = useState<MemoryFileDetail | null>(null)
  useEffect(() => { void memory.file(target.name, target.shared).then(setDetail) }, [memory, target])

  return <div className="memory-file-sheet">
    <ModalHeader title={target.name} />
    <CardForm>
      {memory.error ? <CardSection><p role="alert" className="memory-error">{memory.error}</p></CardSection> : null}
      {detail
        ? <CardSection footer={detail.path}><pre className="memory-file-text">{detail.content ?? 'Deleted.'}</pre></CardSection>
        : <CardSection><p className="memory-muted">Loading…</p></CardSection>}
    </CardForm>
  </div>
}
