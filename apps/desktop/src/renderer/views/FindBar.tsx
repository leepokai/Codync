import { useEffect, useRef } from 'react'
import { IconButton, SearchField } from '../components/Controls'
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
  const bar = useRef<HTMLDivElement>(null)
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
  useEscapeLast(bar, onClose)

  const label = findLabel(query, state, tr('No results'))
  const none = !!query.trim() && state.matches === 0
  return (
    <div ref={bar} className="find-bar" role="search">
      <SearchField value={query} onChange={setQuery} prompt="Find in conversation" inputRef={input} onKeyDown={onInputKey} />
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

/**
 * Escape closes the bar wherever focus is (its field, the composer, the chat), once nothing else took the key: a dialog
 * or menu opened over it (`useEscape` stops the key; the account panel and the composer's reply and `/` list prevent
 * its default), an input method's candidate, or a modal that made the page under the bar inert.
 */
function useEscapeLast(bar: React.RefObject<HTMLElement | null>, onEscape: () => void) {
  const handler = useRef(onEscape)
  handler.current = onEscape
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.isComposing) return
      // Decided once the key has reached every listener: a menu that opened after the bar hears it after this one.
      setTimeout(() => {
        if (!e.defaultPrevented && !bar.current?.closest('[inert]')) handler.current()
      })
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [bar])
}
