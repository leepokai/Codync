import { useEffect } from 'react'
import type { ArtifactCard as Card } from '@shared/artifacts'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { ArtifactFrame } from './ArtifactParts'

/** An artifact in the chat (Grok Bot's artifact card): the top of its newest version over its title. Opens the artifact. */
export function ArtifactCard({ botId, card, open }: { botId: string; card: Card; open: () => void }) {
  const tr = useTr()
  const store = useStore()
  const artifact = store.artifacts.byId.get(card.id)
  const html = store.artifacts.html.get(card.id)
  const title = artifact?.title ?? card.title
  const newest = artifact?.updatedAt
  useEffect(() => store.artifacts.load(store.client, botId, card.id, card.updatedAt), [store, store.client, botId, card.id, card.updatedAt, newest])
  return (
    <button className="artifact-card" onClick={open} aria-label={`${title}, ${tr('Artifact')}`}>
      <div className="artifact-card-preview">{html ? <ArtifactFrame id={card.id} html={html} title={title} preview /> : null}</div>
      <HStack gap={12} className="artifact-card-footer">
        <Icon name="chart.bar.doc.horizontal" size={26} color="var(--secondary)" />
        <VStack shrink>
          <Text size="subheadline" weight="semibold" truncate>{title}</Text>
          <Text size="caption" tone="secondary">{tr('Artifact')}</Text>
        </VStack>
      </HStack>
    </button>
  )
}
