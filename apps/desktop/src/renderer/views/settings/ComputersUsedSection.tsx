import type { ReactNode } from 'react'
import { CardSection } from '../../components/Controls'
import { Text } from '../../components/Text'
import { VStack } from '../../components/Stack'
import { useTr } from '../../lib/use-language'
import type { BotStore } from '../../store/bot-store'
import { useApp } from '../../store/context'
import { SectionHeader } from './parts'

/** The computers this computer can use (the ones it manages over SSH), each with its full settings. */
export function ComputersUsedSection({ store, details }: { store: BotStore; details: (computer: BotStore) => ReactNode }) {
  const tr = useTr()
  const used = useApp().managedStores.filter((s) => s !== store)
  const title = tr('{name} can use').replaceAll('{name}', store.hostName)
  // Empty, it's the same gray card as "can be used by" below.
  if (used.length === 0) return <CardSection title={title}><Text tone="secondary">{tr('None yet.')}</Text></CardSection>
  return (
    <VStack gap={16}>
      <SectionHeader title={title} />
      {used.map((computer) => <div key={computer.computer.id}>{details(computer)}</div>)}
    </VStack>
  )
}
