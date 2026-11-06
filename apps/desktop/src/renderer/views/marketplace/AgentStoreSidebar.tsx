import type { Backend } from '@shared/models'
import { AgentIcon } from '../../components/AgentIcon'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { CATEGORIES, CATEGORY_LABELS } from './market-models'

/**
 * The Extensions sidebar for Agents: the CLIs that run bots on this computer and the
 * catalog's groups, so both are reachable without scrolling past the whole catalog.
 */
export function AgentStoreSidebar({ agents, group, onGroup, onAgent }: {
  agents: Backend[]
  group: string
  onGroup: (group: string) => void
  onAgent: (backend: Backend) => void
}) {
  const tr = useTr()
  return (
    <aside className="market-side" aria-label={tr('Agents and groups')}>
      <Text size="caption" weight="semibold" tone="tertiary" className="market-side-title">{tr('Groups')}</Text>
      <button type="button" className="market-side-row" data-current={group === 'all'} onClick={() => onGroup('all')}>
        <span className="market-side-name">{tr('All agents')}</span>
      </button>
      <button type="button" className="market-side-row" data-current={group === 'featured'} onClick={() => onGroup('featured')}>
        <span className="market-side-name">{tr('Featured')}</span>
      </button>
      {CATEGORIES.map((category) => (
        <button key={category} type="button" className="market-side-row" data-current={group === category} onClick={() => onGroup(category)}>
          <span className="market-side-name">{tr(CATEGORY_LABELS[category]!)}</span>
        </button>
      ))}
      <Text size="caption" weight="semibold" tone="tertiary" className="market-side-title">{tr('Agent CLIs')}</Text>
      {agents.map((backend) => (
        <button key={backend.id} type="button" className="market-side-row" onClick={() => onAgent(backend)}>
          <AgentIcon registry={backend.registry} size={16} color="var(--text)" />
          <span className="market-side-copy">
            <span className="market-side-name">{backend.name}</span>
            {backend.description ? <span className="market-side-spec">{backend.description}</span> : null}
          </span>
        </button>
      ))}
    </aside>
  )
}
