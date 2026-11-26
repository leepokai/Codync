import { useState, type ClipboardEvent, type DragEvent } from 'react'
import type { PickedFile } from '@shared/ipc'
import { useTr } from '../../lib/use-language'
import { useModel } from '../../lib/observable'
import { useStore } from '../../store/context'
import { MAX_FILE_SIZE, type DraftFiles, type OutgoingFile } from '../../store/outgoing-files'
import { pastedFiles, readPicked } from './composer-files'

export interface ComposerFileDraft {
  owner: DraftFiles
  key: string
}

/** Picked, dropped or pasted files; an optional draft keeps them across composer mounts. */
export function useComposerFiles(enabled: boolean, draft?: ComposerFileDraft) {
  const store = useStore()
  const tr = useTr()
  useModel(draft?.owner)
  const [own, setOwn] = useState<OutgoingFile[]>([])
  const files = draft ? draft.owner.get(draft.key) : own
  const setFiles = (next: (list: OutgoingFile[]) => OutgoingFile[]) => {
    if (!draft) return setOwn(next)
    draft.owner.update(draft.key, next)
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
    clear: () => setFiles((list) => list.filter((file) => !files.includes(file))),
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
