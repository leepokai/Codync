import { Button, IconButton, SearchField, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage, usePresence } from '../../components/Overlay'
import { Text } from '../../components/Text'
import { useFolderPicker } from '../../store/use-folder-picker'
import { useTr } from '../../lib/use-language'
import { lastPathComponent } from './drafts'
import './folder-picker.css'

/** The computer is fixed by the bot being edited; never select a path on another host. */
export function FolderPicker({ start, onPick }: { start: string | null; onPick: (path: string) => void }) {
  const tr = useTr()
  const { store, mode, showRecent, listing, selected, setSelected, filter, setFilter,
    error, busy, saving, pathOpen, setPathOpen, input, setInput, online, open, confirm } = useFolderPicker(onPick)
  const presence = usePresence(pathOpen, 180)
  const recent = listing?.recent?.filter(item => `${item.path}`.toLocaleLowerCase().includes(filter.toLocaleLowerCase())) ?? []
  const dirs = listing?.dirs.filter(item => item.name.toLocaleLowerCase().includes(filter.toLocaleLowerCase())) ?? []
  const noRecent = mode === 'recent' && !busy && !listing?.recent?.length
  const browse = <button className="workspace-row workspace-browse" disabled={saving} onClick={() => open(start)}><Icon name="folder" size={16} /><Text grow>{tr('Browse other folders')}</Text><Icon name="chevron.right" size={10} /></button>
  return <SheetPage>
    <ModalHeader title="Choose workspace" />
    <div className="workspace-picker">
      <Text tone="secondary">{tr('Your bot will work in this folder.')}</Text>
      <div className="workspace-computer"><Icon name="desktopcomputer" size={16} /><Text truncate>{store.hello?.name ?? tr('Computer')}</Text><Text tone="secondary">{tr(online ? 'Connected' : 'Disconnected')}</Text></div>
      {noRecent && browse}
      {mode === 'browse' && <div className="workspace-navigation">
        <IconButton title="Recent workspaces" icon="chevron.left" disabled={saving} onClick={showRecent} />
        <nav className="workspace-location" aria-label={tr('Folder path')}>{listing?.ancestors?.length ? listing.ancestors.map(ancestor => <button key={ancestor.path} disabled={saving || busy} onClick={() => open(ancestor.path)}>{ancestor.name}</button>) : <Text tone="secondary" truncate>{listing?.path || tr('Folders')}</Text>}</nav>
        <IconButton title="Parent folder" icon="arrow.up" disabled={busy || saving || listing?.parent == null} onClick={() => open(listing?.parent ?? null)} />
        <IconButton title="Enter folder path" icon="pencil" disabled={saving} onClick={() => { setInput(listing?.path ?? ''); setPathOpen(!pathOpen) }} />
      </div>}
      {presence.mounted && <form className={`workspace-path ${presence.shown ? 'shown' : ''}`} onSubmit={e => { e.preventDefault(); if (input.trim() && !saving) open(input.trim()) }}>
        <input aria-label={tr('Folder path')} placeholder={tr('Paste an absolute path')} value={input} onChange={e => setInput(e.target.value)} autoFocus />
        <IconButton title="Go to folder" icon="arrow.right" disabled={!input.trim() || saving} onClick={() => open(input.trim())} />
      </form>}
      {!noRecent && <>
        <SearchField value={filter} onChange={setFilter} prompt={mode === 'recent' ? 'Search recent workspaces' : 'Filter this folder'} />
        <Text tone="secondary" className="workspace-section-title">{tr(mode === 'recent' ? 'Recent workspaces' : 'Folders')}</Text>
        <div className="workspace-list" key={mode}>
          {busy ? <div className="workspace-empty"><Spinner /></div> : mode === 'recent' ? <>
            {recent.map(item => <button key={item.path} disabled={saving} className={`workspace-row ${selected === item.path ? 'selected' : ''}`} aria-pressed={selected === item.path} onClick={() => setSelected(item.path)}>
              <Icon name="folder" size={18} /><div className="workspace-row-label"><Text truncate>{lastPathComponent(item.path)}</Text><Text tone="secondary" truncate className="workspace-row-path">{item.path}</Text></div>
              {selected === item.path && <Icon name="checkmark" size={14} />}
            </button>)}
            {!recent.length && <Text tone="secondary" className="workspace-empty">{tr(filter ? 'No matching folders.' : 'Choose a folder to see it here next time.')}</Text>}
          </> : <>
            {dirs.map(dir => <button key={dir.path} disabled={saving} className="workspace-row directory" onClick={() => open(dir.path)}><Icon name={dir.isGit ? 'arrow.triangle.branch' : 'folder'} size={16} /><Text truncate grow>{dir.name}</Text><Icon name="chevron.right" size={10} /></button>)}
            {listing && !dirs.length && <Text tone="secondary" className="workspace-empty">{tr(filter ? 'No matching folders.' : 'No subfolders. You can use this folder.')}</Text>}
          </>}
        </div>
      </>}
      {error && <Text tone="danger" role="alert">{error}</Text>}
      {mode === 'recent' && !noRecent && browse}
    </div>
    <div className="workspace-footer"><div className="workspace-row-label"><Text tone="secondary" className="workspace-row-path">{tr(mode === 'recent' ? 'Selected folder' : 'Current folder')}</Text><Text truncate>{selected || tr('Choose a folder')}</Text></div><Button onClick={() => void confirm()} disabled={!selected || busy || saving || !online}>{saving ? <Spinner /> : tr('Use this folder')}</Button></div>
  </SheetPage>
}
