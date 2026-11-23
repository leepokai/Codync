import { IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import type { WorkflowSummary } from './workflow'

/** The open workflows as tabs in the window's bar, the shown one raised, + for a new one. The last tab can't be closed. */
export function WorkflowTabs({ tabs, selectedId, onOpen, onClose, onCreate }: {
  tabs: WorkflowSummary[]
  selectedId: string | null
  onOpen: (id: string) => void
  onClose: (id: string) => void
  onCreate: () => void
}) {
  const tr = useTr()
  return (
    <div className="workflow-tabs" role="tablist" aria-label={tr('Workflows')}>
      {tabs.map((tab) => {
        const on = tab.id === selectedId
        return (
          <div key={tab.id} className={`workflow-tab${on ? ' on' : ''}`}>
            <button type="button" role="tab" aria-selected={on} className="workflow-tab-open" title={tab.title} onClick={() => onOpen(tab.id)}>
              <Icon name="flowchart" size={12} color="var(--secondary)" />
              <Text size="callout" tone="text" truncate>{tab.title}</Text>
            </button>
            {tabs.length > 1 ? (
              <IconButton title={tr('Close {name}').replaceAll('{name}', () => tab.title)} icon="xmark" size={18} className="workflow-tab-close" onClick={() => onClose(tab.id)} />
            ) : null}
          </div>
        )
      })}
      <IconButton title="New workflow" icon="plus" size={24} onClick={onCreate} />
    </div>
  )
}
