import { Button, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import type { ScreenNotice } from '../../lib/screen-state'
import { useTr } from '../../lib/use-language'
import './screen-viewer.css'

/** What shows instead of the picture; `compact` in a small picture (the bot details' Computer tab). */
export function Notice({ icon, text, retry, compact = false }: ScreenNotice & { compact?: boolean }) {
  return (
    <VStack gap={compact ? 8 : 12} className={`screen-viewer-notice${compact ? ' compact' : ''}`}>
      {icon ? <Icon name={icon} size={compact ? 20 : 28} color="var(--secondary)" /> : <Spinner size={compact ? 18 : 22} />}
      <Text size={compact ? 'caption' : 'callout'} tone="secondary">{text}</Text>
      {retry ? <Button kind="secondary" onClick={retry} title="Try again"><Icon name="arrow.clockwise" size={13} /></Button> : null}
    </VStack>
  )
}

/** Why a picture waits for a click: not asked for yet (the bot details' Computer tab) or paused after a while untouched. */
export type ScreenPromptKind = 'start' | 'paused'

const PROMPTS: Record<ScreenPromptKind, string> = { start: 'Click to see the screen', paused: 'Paused. Click to continue.' }

/** In place of a picture that streams only when clicked: the whole area is one button. */
export function ScreenPrompt({ kind, onClick, compact = false }: { kind: ScreenPromptKind; onClick: () => void; compact?: boolean }) {
  const tr = useTr()
  const text = tr(PROMPTS[kind])
  return (
    <button className={`screen-viewer-prompt${compact ? ' compact' : ''}`} aria-label={text} onClick={onClick}>
      <Icon name="play.circle" size={compact ? 26 : 36} color="var(--text)" />
      <Text size={compact ? 'caption' : 'callout'} tone="secondary">{text}</Text>
    </button>
  )
}
