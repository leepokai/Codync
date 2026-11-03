import { useEffect, useState } from 'react'
import type { DirListing } from '@shared/models'
import { CardSection, IconButton, SearchField, Spinner } from './Controls'
import { Icon } from './Icon'
import { HStack, IconSlot } from './Stack'
import { Text } from './Text'
import { ModalHeader, SheetPage } from './Overlay'
import { useStore } from '../store/context'
import { lastPathComponent } from '../lib/path'
import './folder-picker.css'

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e))

/** Browses folders on the host, drilling down in place. The store's computer supplies `listDirs`. */
export function FolderPicker({ start, onPick }: { start: string | null; onPick: (path: string) => void }) {
  // The folders opened so far; the last one is shown.
  const [stack, setStack] = useState<(string | null)[]>([start])
  const [forward, setForward] = useState(true)
  const current = stack[stack.length - 1] ?? null
  return (
    <SheetPage>
      <HStack>
        {stack.length > 1 ? (
          <HStack align="stretch" className="fade-in folder-back">
            <IconButton
              title="Back"
              icon="chevron.left"
              onClick={() => {
                setForward(false)
                setStack((s) => s.slice(0, -1))
              }}
            />
          </HStack>
        ) : null}
        <div className="folder-title">
          <ModalHeader title={current ? lastPathComponent(current) : 'Folders'} />
        </div>
      </HStack>
      <div className="folder-levels">
        <FolderLevel
          key={stack.length}
          path={current}
          forward={forward}
          onPick={onPick}
          onOpen={(path) => {
            setForward(true)
            setStack((s) => [...s, path])
          }}
        />
      </div>
    </SheetPage>
  )
}

/** One folder's subfolders. */
function FolderLevel({ path, forward, onPick, onOpen }: { path: string | null; forward: boolean; onPick: (path: string) => void; onOpen: (path: string) => void }) {
  const store = useStore()
  const [listing, setListing] = useState<DirListing | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [filter, setFilter] = useState('')

  useEffect(() => {
    let live = true
    store
      .listDirs(path)
      .then((l) => live && setListing(l))
      .catch((e: unknown) => live && setError(errorText(e)))
    return () => {
      live = false
    }
  }, [store, path])

  const dirs = listing?.dirs.filter((d) => !filter || d.name.toLowerCase().includes(filter.toLowerCase())) ?? []

  return (
    <div className={`folder-picker level-slide ${forward ? 'from-trailing' : 'from-leading'}`}>
      <div className="folder-level-content">
        <SearchField value={filter} onChange={setFilter} />
        {listing ? (
          <>
            <CardSection footer={listing.path}>
              <button className="press folder-use" onClick={() => onPick(listing.path)}>
                <Icon name="checkmark.circle.fill" size={13} />
                <Text truncate>{`Use “${lastPathComponent(listing.path)}”`}</Text>
              </button>
            </CardSection>
            {dirs.length ? (
              <CardSection>
                {dirs.map((dir) => (
                  <button key={dir.path} className="press folder-row" onClick={() => onOpen(dir.path)}>
                    <IconSlot width={20}>
                      <Icon name={dir.isGit ? 'arrow.triangle.branch' : 'folder'} size={12} color={dir.isGit ? 'var(--accent)' : 'var(--secondary)'} />
                    </IconSlot>
                    <Text tone="text" truncate grow>{dir.name}</Text>
                    <Icon name="chevron.right" size={10} weight="semibold" color="var(--tertiary)" />
                  </button>
                ))}
              </CardSection>
            ) : null}
          </>
        ) : error ? (
          <Text tone="danger">{error}</Text>
        ) : (
          <HStack align="stretch" justify="center">
            <Spinner size={20} />
          </HStack>
        )}
      </div>
    </div>
  )
}
