import { useEffect, useMemo, useState } from 'react'
import { Button, CardSection, ChoicePicker, DropdownMenu, IconButton, SearchField } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog, ModalHeader, Sheet } from '../../components/Overlay'
import { useModel } from '../../lib/observable'
import { useStore } from '../../store/context'
import { MemoryStore, type MemoryDraft, type MemoryFact } from '../../store/memory-store'
import './memory.css'

const FILTERS = [
  { id: 'all', label: 'All' }, { id: 'profile', label: 'About you' },
  { id: 'pinned', label: 'Pinned' }, { id: 'review', label: 'Needs review' },
]

export function MemoryCard({ botId }: { botId: string }) {
  const host = useStore()
  const memory = useModel(useMemo(() => new MemoryStore(botId, () => host.ready()), [botId, host]))
  const [draft, setDraft] = useState<MemoryDraft | null>(null)
  const [showDetail, setShowDetail] = useState(false)
  const [backupMode, setBackupMode] = useState<'export' | 'import' | null>(null)
  const [importText, setImportText] = useState('')
  const [confirmClear, setConfirmClear] = useState(false)
  useEffect(() => { void memory.load() }, [memory])

  const edit = (fact?: MemoryFact) => setDraft({
    id: fact?.id, title: fact?.title ?? '', content: fact?.content ?? '',
    scope: fact?.scope ?? 'project', memoryType: fact?.memoryType ?? 'learning', topicKey: fact?.topicKey ?? '',
  })

  return <>
    <CardSection title="Memory" accessory={<div className="memory-header-actions">
      <IconButton title="Add memory" icon="plus" onClick={() => edit()} disabled={memory.busy} />
      <DropdownMenu className="icon-button memory-menu" title="More" items={() => [
        { title: 'Refresh', icon: 'arrow.clockwise', action: () => void memory.load() },
        { title: 'Export', icon: 'square.and.arrow.up', action: () => { setBackupMode('export'); void memory.export() } },
        { title: 'Import', icon: 'arrow.down.circle', action: () => setBackupMode('import') },
        { title: 'Forget everything', icon: 'trash', destructive: true, divider: true, action: () => setConfirmClear(true) },
      ]}><Icon name="ellipsis" size={14} weight="medium" /></DropdownMenu>
    </div>}>
      <div className="memory-search">
        <SearchField prompt="Search memories" value={memory.query} onChange={q => memory.search(q)} />
        <ChoicePicker selection={memory.filter} options={FILTERS} fill="var(--bubble-agent)" onChange={f => memory.search(memory.query, f)} />
      </div>
      {memory.error && <p role="alert" className="memory-error">{memory.error}</p>}
      {!memory.loaded && !memory.error && <p className="memory-muted">Loading memory…</p>}
      {memory.loaded && !memory.facts.length && <div className="memory-empty">
        <Icon name="brain" size={20} color="var(--tertiary)" />
        <span>{memory.query || memory.filter !== 'all' ? 'No matching memories.' : 'Nothing yet. Important details are remembered as you chat.'}</span>
      </div>}
      {memory.facts.map(fact => <div className="memory-item fade-in" key={fact.id}>
        <button className="press memory-content" onClick={() => edit(fact)}>
          <strong>{fact.pinned && <Icon name="pin.fill" size={10} color="var(--secondary)" />}{fact.title}</strong>
          <span>{fact.content}</span>
          <small>{fact.kind === 'profile' ? 'About you' : fact.memoryType} · {new Date(fact.createdAt).toLocaleDateString()} · {fact.revisionCount} revisions</small>
        </button>
        <DropdownMenu className="icon-button memory-menu" title="Memory actions" items={() => [
          { title: fact.pinned ? 'Unpin' : 'Pin', icon: fact.pinned ? 'pin.slash' : 'pin', action: () => void memory.mutate('pinMemory', { id: fact.id, pinned: !fact.pinned }) },
          { title: 'History', icon: 'clock.arrow.circlepath', action: () => { setShowDetail(true); void memory.inspect(fact) } },
          ...(fact.reviewAfter ? [{ title: 'Mark reviewed', icon: 'checkmark.circle', action: () => void memory.mutate('reviewMemory', { id: fact.id }) }] : []),
          { title: 'Forget', icon: 'trash', destructive: true, divider: true, action: () => void memory.mutate('forgetMemory', { id: fact.id }) },
        ]}><Icon name="ellipsis" size={14} weight="medium" /></DropdownMenu>
      </div>)}
      {memory.nextOffset !== null && <button className="press memory-more" onClick={() => void memory.load(true)}>Load more ({memory.total})</button>}
    </CardSection>
    <Sheet open={draft !== null} onClose={() => setDraft(null)} width={560}>
      <ModalHeader title={draft?.id ? 'Edit memory' : 'Add memory'} />
      {draft && <div className="memory-editor">
        <label>Title<input value={draft.title} onChange={e => setDraft({ ...draft, title: e.target.value })} /></label>
        <label>Memory<textarea rows={8} value={draft.content} onChange={e => setDraft({ ...draft, content: e.target.value })} /></label>
        <div className="memory-actions">{['project', 'personal', 'global'].map(scope => <button className="press" key={scope} aria-pressed={draft.scope === scope} onClick={() => setDraft({ ...draft, scope })}>{scope === 'personal' ? 'About you' : scope === 'global' ? 'General' : 'Work'}</button>)}</div>
        <label>Category<input value={draft.memoryType} onChange={e => setDraft({ ...draft, memoryType: e.target.value })} /></label>
        <label>Topic (optional)<input value={draft.topicKey} onChange={e => setDraft({ ...draft, topicKey: e.target.value })} /></label>
        {memory.error && <p role="alert" className="memory-error">{memory.error}</p>}
        <Button disabled={memory.busy || !draft.title.trim() || !draft.content.trim()} onClick={() => { void memory.mutate('saveMemory', draft).then(saved => { if (saved) setDraft(null) }) }}>{memory.busy ? 'Saving…' : 'Save'}</Button>
      </div>}
    </Sheet>
    <Sheet open={showDetail} onClose={() => setShowDetail(false)} width={620}>
      <ModalHeader title="Memory history" />
      <div className="memory-editor">
        {memory.error && <p className="memory-error">{memory.error}</p>}
        {memory.detail ? <><pre className="memory-history">{memory.detail.history.result}</pre>
          {memory.detail.history.history_cursor && <button className="press" onClick={() => void memory.inspect(memory.detail!.fact, memory.detail!.history.history_cursor)}>Older versions</button>}
          <h4>Session timeline</h4><pre className="memory-history">{memory.detail.timeline.result}</pre></> : <p>Loading history…</p>}
      </div>
    </Sheet>
    <Sheet open={backupMode !== null} onClose={() => setBackupMode(null)} width={620}>
      <ModalHeader title={backupMode === 'export' ? 'Export memories' : 'Import memories'} />
      <div className="memory-editor">
        <p>{backupMode === 'export' ? 'Copy this Engram backup to keep or import your memories.' : 'Paste an Engram JSON backup. Import merges memories and preserves newer changes. A backup is saved first.'}</p>
        <textarea aria-label="Memory backup" rows={14} readOnly={backupMode === 'export'} value={backupMode === 'export' ? memory.backup : importText} onChange={e => setImportText(e.target.value)} />
        {memory.error && <p className="memory-error">{memory.error}</p>}
        {backupMode === 'import' && <Button disabled={memory.busy || !importText.trim()} onClick={() => { void memory.import(importText).then(ok => { if (ok) setBackupMode(null) }) }}>Import</Button>}
      </div>
    </Sheet>
    <Dialog open={confirmClear} title="Forget everything this bot remembers?" actions={[{ title: 'Forget everything', destructive: true, action: () => { void memory.mutate('clearMemory') } }]} onClose={() => setConfirmClear(false)} />
  </>
}
