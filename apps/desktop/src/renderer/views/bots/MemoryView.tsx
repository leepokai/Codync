import { useEffect, useMemo, useState } from 'react'
import { Button, CardSection } from '../../components/Controls'
import { Dialog, ModalHeader, Sheet } from '../../components/Overlay'
import { useModel } from '../../lib/observable'
import { useStore } from '../../store/context'
import { MemoryStore, type MemoryDraft, type MemoryFact } from '../../store/memory-store'
import './memory.css'

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
    <CardSection title="Memory" accessory={<button className="press" onClick={() => edit()} disabled={memory.busy}>Add</button>}>
      <div className="memory-actions">
        <input aria-label="Search memories" placeholder="Search memories" value={memory.query} onChange={e => memory.search(e.target.value)} />
        <button className="press" onClick={() => void memory.load()}>Refresh</button>
      </div>
      <div className="memory-actions" role="group" aria-label="Memory filter">
        {(['all', 'profile', 'pinned', 'review'] as const).map(filter => <button key={filter} className="press" aria-pressed={memory.filter === filter}
          onClick={() => memory.search(memory.query, filter)}>{({ all: 'All', profile: 'About you', pinned: 'Pinned', review: 'Needs review' })[filter]}</button>)}
      </div>
      {memory.error && <p role="alert" className="memory-error">{memory.error}</p>}
      {!memory.loaded && !memory.error && <p>Loading memory…</p>}
      {memory.loaded && !memory.facts.length && <p className="memory-muted">{memory.query || memory.filter !== 'all' ? 'No matching memories.' : 'Nothing yet. Important details are remembered as you chat.'}</p>}
      {memory.facts.map(fact => <div className="memory-item fade-in" key={fact.id}>
        <button className="press memory-content" onClick={() => edit(fact)}>
          <strong>{fact.pinned ? '● ' : ''}{fact.title}</strong>
          <span>{fact.content}</span>
          <small>{fact.kind === 'profile' ? 'About you' : fact.memoryType} · {new Date(fact.createdAt).toLocaleDateString()} · {fact.revisionCount} revisions</small>
        </button>
        <div className="memory-actions">
          <button className="press" onClick={() => void memory.mutate('pinMemory', { id: fact.id, pinned: !fact.pinned })} disabled={memory.busy}>{fact.pinned ? 'Unpin' : 'Pin'}</button>
          <button className="press" onClick={() => { setShowDetail(true); void memory.inspect(fact) }}>History</button>
          {fact.reviewAfter && <button className="press" onClick={() => void memory.mutate('reviewMemory', { id: fact.id })} disabled={memory.busy}>Mark reviewed</button>}
          <button className="press" onClick={() => void memory.mutate('forgetMemory', { id: fact.id })} disabled={memory.busy}>Forget</button>
        </div>
      </div>)}
      {memory.nextOffset !== null && <button className="press" onClick={() => void memory.load(true)}>Load more ({memory.total})</button>}
      <div className="memory-actions">
        <button className="press" onClick={() => { setBackupMode('export'); void memory.export() }}>Export</button>
        <button className="press" onClick={() => setBackupMode('import')}>Import</button>
        <button className="press" onClick={() => setConfirmClear(true)} disabled={memory.busy}>Forget everything</button>
      </div>
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
