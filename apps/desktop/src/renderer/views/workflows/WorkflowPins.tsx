import { useState } from 'react'
import { Icon } from '../../components/Icon'
import { AnchoredMenu, type MenuItem } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { usePref } from '../../lib/prefs'
import type { BotStore } from '../../store/bot-store'
import { pinWorkflow, workflowPins } from './pins'
import { summaryLine } from './workflow'

/** Pinned workflows above this computer's bots. The pin list is on this device only. */
export function WorkflowPins({ store, compact, activeId, onOpen }: {
  store: BotStore
  compact: boolean
  activeId: string | null
  onOpen: (id: string) => void
}) {
  useModel(store)
  const [ids] = usePref(workflowPins(store.computer.id))
  const [menu, setMenu] = useState<{ id: string; x: number; y: number } | null>(null)
  const rows = ids.flatMap((id) => {
    const summary = store.workflows.find((item) => item.id === id)
    return summary ? [{ id, summary }] : []
  })
  if (!rows.length) return null
  const items = (): MenuItem[] => menu ? [{ title: 'Unpin', action: () => pinWorkflow(store.computer.id, menu.id, false) }] : []
  return (
    <>
      {rows.map(({ id, summary }) => (
        <button
          key={id}
          type="button"
          className={`roster-row workflow-pin ${compact ? 'compact' : ''} ${activeId === id ? 'selected' : ''}`}
          onClick={() => onOpen(id)}
          onContextMenu={(event) => {
            event.preventDefault()
            setMenu({ id, x: event.clientX, y: event.clientY })
          }}
        >
          <HStack gap={compact ? 0 : 10} justify={compact ? 'center' : 'flex-start'}>
            <Icon name="flowchart" size={18} />
            {compact ? null : (
              <VStack gap={2}>
                <Text size="body" weight="medium" truncate>{summary.title}</Text>
                {summaryLine(summary) ? <Text size="callout" tone="secondary" truncate>{summaryLine(summary)}</Text> : null}
              </VStack>
            )}
          </HStack>
        </button>
      ))}
      <AnchoredMenu open={menu !== null} point={menu} items={items} onClose={() => setMenu(null)} />
    </>
  )
}
