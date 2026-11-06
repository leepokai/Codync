import type { ReactNode } from 'react'
import type { Backend } from '@shared/models'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { AgentTile, NodeNotice } from './AgentSheet'
import type { ConnectorCatalog } from './connector-catalog'
import { EXTENSION_SECTIONS, type ExtensionSection, type MarketConnector, type MarketSkill } from './market-models'
import { ItemGrid, LoadMoreRow, MarketRow, ServiceLogo, SkeletonGrid, SkillGlyph, TileIcon } from './pieces'

/** One tab of Extensions: the line saying where its items come from, the tab, and an error under it. */
export function MarketPanel({ section, hostName, error, children }: { section: ExtensionSection; hostName: string; error: string | null; children: ReactNode }) {
  const tr = useTr()
  const source = EXTENSION_SECTIONS.find((s) => s.id === section)?.source ?? ''
  return (
    <div className="market-panel">
      <Text size="footnote" tone="tertiary">{tr(source).replaceAll('{name}', hostName)}</Text>
      {children}
      {error ? <Text size="footnote" tone="danger">{tr(error)}</Text> : null}
    </div>
  )
}

export function AgentsPanel({ agents, query, needsNode, onAddNode, onAgent }: {
  agents: Backend[]
  query: string
  needsNode: boolean
  onAddNode: () => void
  onAgent: (backend: Backend) => void
}) {
  const tr = useTr()
  return (
    <>
      {needsNode ? <NodeNotice onAdd={onAddNode} /> : null}
      <div className="market-panel-scroll fade-in">
        {agents.length === 0 ? (
          <Text tone="secondary">{query ? tr('No agents match “{query}”.').replaceAll('{query}', query) : tr('No agents yet.')}</Text>
        ) : (
          <div className="agent-grid">
            {agents.map((b) => <AgentCard key={b.id} backend={b} onClick={() => onAgent(b)} />)}
          </div>
        )}
      </div>
    </>
  )
}

export function ConnectorsPanel({ catalog, query, onAddConnector, onInstall, onView }: {
  catalog: ConnectorCatalog<MarketConnector>
  query: string
  onAddConnector: () => void
  onInstall: (connector: MarketConnector) => void
  onView: (connector: MarketConnector) => void
}) {
  const tr = useTr()
  const rows = catalog.items.items
  return (
    <>
      <MarketRow
        title={tr('Custom connector')}
        subtitle={tr('Any MCP server: a command or a URL')}
        added={false}
        addLabel="New"
        icon={<TileIcon name="point.3.connected.trianglepath.dotted" />}
        onAdd={onAddConnector}
      />
      <div className="market-panel-scroll fade-in">
        {catalog.loading ? (
          <SkeletonGrid />
        ) : rows.length === 0 && !catalog.hasMore ? (
          <Text tone="secondary">{query ? tr('No connectors match “{query}”.').replaceAll('{query}', query) : tr(`Couldn't load connectors.`)}</Text>
        ) : (
          <>
            <ItemGrid>
              {rows.map((c) => (
                <MarketRow
                  key={c.name}
                  title={c.title}
                  subtitle={c.description ?? c.name}
                  added={c.installed}
                  icon={<ServiceLogo website={c.website} name={c.title} registryName={c.name} />}
                  onAdd={() => onInstall(c)}
                  onOpen={c.installed ? undefined : () => onView(c)}
                />
              ))}
            </ItemGrid>
            {catalog.cursor !== null ? <LoadMoreRow cursor={catalog.cursor} failed={catalog.error !== null} onLoad={() => void catalog.loadMore()} /> : null}
          </>
        )}
      </div>
    </>
  )
}

export function SkillsPanel({ skills, loading, query, installedSkillIds, busySkill, onWriteSkill, onInstallSkill, onViewSkill }: {
  skills: MarketSkill[]
  loading: boolean
  query: string
  installedSkillIds: string[]
  busySkill: string | null
  onWriteSkill: () => void
  onInstallSkill: (skill: MarketSkill) => void
  onViewSkill: (skill: MarketSkill) => void
}) {
  const tr = useTr()
  return (
    <>
      <MarketRow
        title={tr('Your own skill')}
        subtitle={tr('Write instructions a bot can follow')}
        added={false}
        addLabel="New"
        icon={<TileIcon name="square.and.pencil" />}
        onAdd={onWriteSkill}
      />
      <div className="market-panel-scroll fade-in">
        {loading ? (
          <SkeletonGrid />
        ) : skills.length === 0 ? (
          query ? <Text tone="secondary">{tr('No skills match “{query}”.').replaceAll('{query}', query)}</Text> : null
        ) : (
          <ItemGrid>
            {skills.map((s) => (
              <MarketRow
                key={s.source}
                title={s.name}
                subtitle={s.description}
                added={s.installed || installedSkillIds.includes(s.source.toLowerCase())}
                busy={busySkill === s.source}
                icon={<SkillGlyph />}
                onAdd={() => onInstallSkill(s)}
                onOpen={() => onViewSkill(s)}
              />
            ))}
          </ItemGrid>
        )}
      </div>
    </>
  )
}

function AgentCard({ backend, onClick }: { backend: Backend; onClick: () => void }) {
  const tr = useTr()
  const status = backend.installed !== true && backend.curated === true ? 'Not installed'
    : backend.node === 'missing' ? 'Needs Node.js'
    : backend.installed !== true ? 'Sets up on first use'
    : backend.signedIn === false ? 'Not signed in'
    : 'Installed'
  return (
    <button className="agent-card" title={backend.name} onClick={onClick}>
      <AgentTile registry={backend.registry ?? backend.id} icon={24} tile={40} radius={11} />
      <VStack as="span" align="flex-start" gap={3} shrink>
        <Text size="subheadline" weight="semibold" tone="text" truncate className="agent-card-line">{backend.name}</Text>
        <Text size="caption" tone="secondary" truncate className="agent-card-line">{tr(status)}</Text>
      </VStack>
    </button>
  )
}
