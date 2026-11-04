import { useEffect, useRef } from 'react'
import { IconButton } from '../components/Controls'
import { Icon } from '../components/Icon'
import { Text } from '../components/Text'
import { findLabel } from '../lib/find-label'
import { useTr } from '../lib/use-language'
import { useConversationFind } from './use-conversation-find'
import './find-bar.css'

/** The transcript the find searches: the conversation on screen, not the bot list beside it. */
const transcript = () => document.querySelector<HTMLElement>('[data-find-root]')

/** Find in the open conversation (⌘F): a small bar over its top right; the matches are highlighted in the chat itself. */
export function FindBar({ focusKey, onClose }: { focusKey: number; onClose: () => void }) {
  const tr = useTr()
  const input = useRef<HTMLInputElement>(null)
  const { query, setQuery, state, step } = useConversationFind(transcript)

  // ⌘F again puts the cursor back in the field with the text selected, ready to be replaced.
  useEffect(() => {
    input.current?.focus()
    input.current?.select()
  }, [focusKey])

  useEffect(
    () =>
      window.codync.app.onCommand((c) => {
        if (c.kind === 'findStep') step(c.forward)
      }),
    [step],
  )

  const onInputKey = (e: React.KeyboardEvent) => {
    if (e.nativeEvent.isComposing || e.key !== 'Enter') return
    e.preventDefault()
    step(!e.shiftKey)
  }
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key !== 'Escape') return
    e.preventDefault()
    e.stopPropagation()
    onClose()
  }

  const label = findLabel(query, state, tr('No results'))
  const none = !!query.trim() && state.matches === 0
  const title = tr('Find in conversation')
  return (
    <div className="find-bar" role="search" onKeyDown={onKey}>
      <Icon name="magnifyingglass" size={13} color="var(--secondary)" />
      <input ref={input} value={query} placeholder={title} aria-label={title} spellCheck={false} onChange={(e) => setQuery(e.target.value)} onKeyDown={onInputKey} />
      {label ? (
        <Text size="callout" tone={none ? 'attention' : 'secondary'} tabular className="find-count">
          {label}
        </Text>
      ) : null}
      <IconButton title="Previous match" icon="chevron.up" size={24} disabled={state.matches === 0} onClick={() => step(false)} />
      <IconButton title="Next match" icon="chevron.down" size={24} disabled={state.matches === 0} onClick={() => step(true)} />
      <IconButton title="Close" icon="xmark" size={24} onClick={onClose} />
    </div>
  )
}
