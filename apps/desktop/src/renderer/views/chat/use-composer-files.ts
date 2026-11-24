import { useState, type ClipboardEvent, type DragEvent } from 'react'
import type { PickedFile } from '@shared/ipc'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { MAX_FILE_SIZE, type OutgoingFile } from '../../store/outgoing-files'
import { pastedFiles, readPicked } from './composer-files'

/** The files of boxes that keep theirs beyond the box (`keep`), by key; memory only. */
const kept = new Map<string, OutgoingFile[]>()

/**
 * The files a message box sends with its next message: picked (`pick`), dropped on the box (`drop`) or pasted
 * (`paste`). Nothing is taken while `enabled` is false. A file over 100 MB is left out and says so. With `keep`
 * they are kept by that key beyond the box, so the box mounted again (a new thread's, as its bot changes) has them.
 */
export function useComposerFiles(enabled: boolean, keep?: string) {
  const store = useStore()
  const tr = useTr()
  const [files, setOwn] = useState<OutgoingFile[]>(() => (keep && kept.get(keep)) || [])
  // Kept ones are changed outside React's updater, so a send that ends after the box is gone still clears them.
  const setFiles = (next: (list: OutgoingFile[]) => OutgoingFile[]) => {
    if (!keep) return setOwn(next)
    const changed = next(kept.get(keep) ?? [])
    kept.set(keep, changed)
    setOwn(changed)
  }
  const append = (incoming: PickedFile[]) => {
    if (!enabled) return
    const accepted: OutgoingFile[] = []
    for (const f of incoming) {
      if (f.data.length > MAX_FILE_SIZE) {
        store.setError(tr('{name} is larger than 100 MB.').replaceAll('{name}', () => f.name))
        continue
      }
      accepted.push({ id: crypto.randomUUID(), name: f.name, data: f.data })
    }
    setFiles((list) => [...list, ...accepted])
  }
  return {
    files,
    clear: () => setFiles(() => []),
    remove: (id: string) => setFiles((list) => list.filter((x) => x.id !== id)),
    pick: () => void window.codync.app.pickFiles().then(append),
    drop: {
      onDragOver: (e: DragEvent) => {
        if (enabled && e.dataTransfer.types.includes('Files')) e.preventDefault()
      },
      onDrop: (e: DragEvent) => {
        if (!enabled) return
        e.preventDefault()
        void readPicked([...e.dataTransfer.files]).then(append)
      },
    },
    /** ⌘V with a copied file or image attaches it (the field alone would paste a name); plain text is left to the field. */
    paste: (e: ClipboardEvent) => {
      const types = [...e.clipboardData.types]
      if (!enabled || !types.includes('Files')) return
      if (types.includes('text/plain') && !e.clipboardData.files.length) return
      e.preventDefault()
      void pastedFiles(window.codync.app.readClipboardFiles, [...e.clipboardData.files]).then(append)
    },
  }
}
