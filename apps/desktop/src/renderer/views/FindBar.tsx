import { useEffect, useRef, useState } from 'react'
import type { FindResult } from '@shared/ipc'
import { IconButton } from '../components/Controls'
import { Icon } from '../components/Icon'
import { Text } from '../components/Text'
import { findLabel } from '../lib/find-label'
import { useTr } from '../lib/use-language'
import './find-bar.css'

/** Find in the open conversation (⌘F): a small bar over its top right; the matches are highlighted in the page itself. */
export function FindBar({ focusKey, onClose }: { focusKey: number; onClose: () => void }) {
  const tr = useTr()
  const input = useRef<HTMLInputElement>(null)
  const [query, setQuery] = useState('')
  const [result, setResult] = useState<FindResult | null>(null)
  const searched = useRef('')

  const step = (forward: boolean) => {
    if (!query) return
    // The same text steps to the next match; new text starts over from the first.
    window.codync.find.search(query, forward, searched.current === query)
    searched.current = query
  }

  // ⌘F again puts the cursor back in the field with the text selected, ready to be replaced.
  useEffect(() => {
    input.current?.focus()
    input.current?.select()
  }, [focusKey])

  useEffect(() => {
    if (!query) {
      searched.current = ''
      setResult(null)
      window.codync.find.stop()
      return
    }
    window.codync.find.search(query, true, false)
    searched.current = query
  }, [query])

  useEffect(
    () =>
      window.codync.find.onResult((r) => {
        setResult(r)
        // Painting a match can take the focus from the field.
        if (document.activeElement !== input.current) input.current?.focus({ preventScroll: true })
      }),
    [],
  )
  useEffect(
    () =>
      window.codync.app.onCommand((c) => {
        if (c.kind === 'findStep') step(c.forward)
      }),
  )
  useEffect(() => () => window.codync.find.stop(), [])

  const onKey = (e: React.KeyboardEvent) => {
    if (e.nativeEvent.isComposing) return
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onClose()
    } else if (e.key === 'Enter') {
      e.preventDefault()
      step(!e.shiftKey)
    }
  }

  const label = findLabel(query, result, tr('No results'))
  const none = !!query.trim() && result?.matches === 0
  return (
    <div className="find-bar" role="search" onKeyDown={onKey}>
      <Icon name="magnifyingglass" size={13} color="var(--secondary)" />
      <input ref={input} value={query} placeholder={tr('Find in conversation')} aria-label={tr('Find in conversation')} spellCheck={false} onChange={(e) => setQuery(e.target.value)} />
      {label ? (
        <Text size="callout" tone={none ? 'attention' : 'secondary'} tabular className="find-count">
          {label}
        </Text>
      ) : null}
      <IconButton title="Previous match" icon="chevron.up" size={24} disabled={!result?.matches} onClick={() => step(false)} />
      <IconButton title="Next match" icon="chevron.down" size={24} disabled={!result?.matches} onClick={() => step(true)} />
      <IconButton title="Close" icon="xmark" size={24} onClick={onClose} />
    </div>
  )
}
