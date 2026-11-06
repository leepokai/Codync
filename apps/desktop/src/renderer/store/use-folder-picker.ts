import { useEffect, useRef, useState } from 'react'
import type { DirListing } from '@shared/models'
import { useStore } from './context'

/** Keeps asynchronous browsing and confirmation out of the renderer. */
export function useFolderPicker(onPick: (path: string) => void) {
  const store = useStore()
  const [requestId, setRequestId] = useState(0)
  const [mode, setMode] = useState<'recent' | 'browse'>('recent')
  const [listing, setListing] = useState<DirListing | null>(null)
  const [path, setPath] = useState<string | null>(null)
  const [selected, setSelected] = useState('')
  const [filter, setFilter] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [saving, setSaving] = useState(false)
  const [pathOpen, setPathOpen] = useState(false)
  const [input, setInput] = useState('')
  const generation = useRef(0)
  const online = store.connection.kind === 'online'
  useEffect(() => {
    const request = ++generation.current
    setBusy(true)
    setError(null)
    setListing(null)
    store.listDirs(path).then(result => {
      if (generation.current !== request) return
      setListing(result)
      if (mode === 'browse') setSelected(result.path)
    }).catch(e => { if (generation.current === request) setError(errorText(e)) })
      .finally(() => { if (generation.current === request) setBusy(false) })
    return () => { generation.current++ }
  }, [store, path, mode, online, requestId])
  const open = (next: string | null) => {
    setSelected('')
    setFilter('')
    setPath(next)
    setMode('browse')
    setPathOpen(false)
    setRequestId(id => id + 1)
  }
  const showRecent = () => {
    setMode('recent')
    setPath(null)
    setFilter('')
    setSelected('')
    setPathOpen(false)
    setRequestId(id => id + 1)
  }
  const confirm = async () => {
    if (!selected || saving || !online || busy) return
    const request = generation.current
    setSaving(true)
    setError(null)
    try {
      // Absence of `recent` means an older host: validate with its existing browsing API.
      const result = listing?.recent ? await store.selectWorkspace(selected) : await store.listDirs(selected)
      if (request === generation.current) onPick(result.path)
    } catch (e) { if (request === generation.current) setError(errorText(e)) }
    finally { setSaving(false) }
  }
  return { store, mode, showRecent, listing, selected, setSelected, filter, setFilter,
    error, busy, saving, pathOpen, setPathOpen, input, setInput, online, open, confirm }
}

function errorText(error: unknown) { return error instanceof Error ? error.message : String(error) }
