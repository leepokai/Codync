import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { CATEGORIES, CATEGORY_LABELS } from './market-models'

/** The Extensions sidebar for Agents: the catalog's groups, reachable without scrolling the catalog. */
export function AgentStoreSidebar({ group, onGroup }: {
  group: string
  onGroup: (group: string) => void
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
    </aside>
  )
}
