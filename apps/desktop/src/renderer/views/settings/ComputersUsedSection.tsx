import type { ReactNode } from 'react'
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
  return (
    <VStack gap={16}>
      <SectionHeader title={tr('{name} can use').replaceAll('{name}', store.hostName)} />
      {used.length === 0 ? <Text tone="secondary">{tr('None yet.')}</Text> : used.map((computer) => <div key={computer.computer.id}>{details(computer)}</div>)}
    </VStack>
  )
}
