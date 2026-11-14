import { useEffect, useMemo, useState } from 'react'
import type { BotDraft } from '@shared/models'
import { Icon } from '../../components/Icon'
import { ModalHeader } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { encodeTemplate } from '../../lib/bot-template'
import { useTr } from '../../lib/use-language'
import './bots.css'

/** A portable settings snapshot, with no conversation identity or runtime state. */
export function BotTemplateView({ draft }: { draft: BotDraft }) {
  const tr = useTr()
  const encoded = useMemo(() => encodeTemplate(draft), [draft])
  const [copied, setCopied] = useState(false)

  const copy = () => {
    void navigator.clipboard.writeText(encoded).then(() => setCopied(true))
  }

  // Return copies (the default action).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Enter' && !e.isComposing) {
        e.preventDefault()
        copy()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

  return (
    <VStack className="bot-template">
      <ModalHeader icon="square.on.square" title={tr(`Create template`)} subtitle={draft.name} />
      <VStack gap={16} className="bot-template-body">
        <Text size="body" tone="secondary">{tr(`Copy this bot’s settings to reuse as a template. Conversation history and conversation ID are excluded.`)}</Text>
        <div className="bot-template-code">
          <pre className="selectable">
            {encoded}
          </pre>
        </div>
        <HStack gap={8}>
          <Text size="caption" tone="secondary" grow>{tr(`Includes workspace preferences and agent configuration.`)}</Text>
          <button className="press bot-template-copy" onClick={copy}>
            <Icon key={copied ? 'checkmark' : 'square.on.square'} name={copied ? 'checkmark' : 'square.on.square'} size={13} weight="semibold" className="fade-in" />
            {tr(copied ? 'Copied' : 'Copy template')}
          </button>
        </HStack>
      </VStack>
    </VStack>
  )
}
