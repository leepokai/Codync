import type { ReactNode } from 'react'
import type { Backend } from '@shared/models'
import { CharacterAvatar } from '../../components/Avatar'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { AgentTile, NodeNotice } from './AgentSheet'
import { ComposioSection } from './ComposioSection'
import type { ConnectorCatalog } from './connector-catalog'
import { CATEGORIES, CATEGORY_LABELS, EXTENSION_SECTIONS, type ExtensionSection, type MarketAgent, type MarketApp, type MarketConnector, type MarketSkill } from './market-models'
import { ItemGrid, LoadMoreRow, MarketRow, MarketSection, ServiceLogo, SkeletonGrid, SkillGlyph, TileIcon } from './pieces'

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

/** The featured items of a kind first, then each category, each in its own titled group. */
function grouped<T extends { category: string; featured: boolean }>(items: T[]): { title: string; items: T[] }[] {
  const featured = items.filter((i) => i.featured)
  const groups = CATEGORIES.map((category) => ({ title: CATEGORY_LABELS[category], items: items.filter((i) => !i.featured && i.category === category) }))
  return [{ title: 'Featured', items: featured }, ...groups].filter((g) => g.items.length > 0)
}

export function AppsPanel({ apps, query, catalog, added, onAddApp, onViewApp, onAddConnector, onInstallConnector, onViewConnector, children }: {
  apps: MarketApp[]
  query: string
  catalog: ConnectorCatalog<MarketConnector>
  added: string[]
  onAddApp: (app: MarketApp) => void
  onViewApp: (app: MarketApp) => void
  onAddConnector: () => void
  onInstallConnector: (connector: MarketConnector) => void
  onViewConnector: (connector: MarketConnector) => void
  children?: ReactNode
}) {
  const tr = useTr()
  const rows = catalog.items.items
  return (
    <div className="market-panel-scroll fade-in">
      {apps.length ? (
        grouped(apps).map((group) => (
          <MarketSection key={group.title} title={tr(group.title)}>
            <ItemGrid>
              {group.items.map((app) => (
                <MarketRow
                  key={app.id}
                  title={app.name}
                  subtitle={app.tagline}
                  added={added.includes(app.id) || app.installed}
                  icon={<ServiceLogo website={app.website} name={app.name} />}
                  onAdd={() => onAddApp(app)}
                  onOpen={() => onViewApp(app)}
                />
              ))}
            </ItemGrid>
          </MarketSection>
        ))
      ) : query ? (
        <Text tone="secondary">{tr('No apps match “{query}”.').replaceAll('{query}', query)}</Text>
      ) : null}
      <ComposioSection query={query} />
      <MarketSection title={tr('Custom connector')}>
        <MarketRow
          title={tr('Any MCP server')}
          subtitle={tr('A command or a URL')}
          added={false}
          addLabel="New"
          icon={<TileIcon name="point.3.connected.trianglepath.dotted" />}
          onAdd={onAddConnector}
        />
      </MarketSection>
      <MarketSection title={tr('MCP Registry')}>
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
                  onAdd={() => onInstallConnector(c)}
                  onOpen={c.installed ? undefined : () => onViewConnector(c)}
                />
              ))}
            </ItemGrid>
            {catalog.cursor !== null ? <LoadMoreRow cursor={catalog.cursor} failed={catalog.error !== null} onLoad={() => void catalog.loadMore()} /> : null}
          </>
        )}
      </MarketSection>
      {children}
    </div>
  )
}

export function MarketAgentsPanel({ agents, query, added, onAdd, onView }: {
  agents: MarketAgent[]
  query: string
  added: string[]
  onAdd: (agent: MarketAgent) => void
  onView: (agent: MarketAgent) => void
}) {
  const tr = useTr()
  return (
    <div className="market-panel-scroll fade-in">
      {agents.length ? (
        grouped(agents).map((group) => (
          <MarketSection key={group.title} title={tr(group.title)}>
            <ItemGrid>
              {group.items.map((agent) => (
                <MarketRow
                  key={agent.id}
                  title={agent.name}
                  subtitle={agent.title}
                  added={added.includes(agent.id) || agent.installed}
                  icon={<CharacterAvatar shape={agent.avatarShape} color={agent.avatarColor} size={46} />}
                  onAdd={() => onAdd(agent)}
                  onOpen={() => onView(agent)}
                />
              ))}
            </ItemGrid>
          </MarketSection>
        ))
      ) : (
        <Text tone="secondary">{tr('No agents match “{query}”.').replaceAll('{query}', query)}</Text>
      )}
    </div>
  )
}

/** Harness agents (Claude, Codex…): the CLIs that run the bots. */
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
  const catalogued = skills.filter((s) => s.category)
  const other = skills.filter((s) => !s.category)
  return (
    <div className="market-panel-scroll fade-in">
      <MarketRow
        title={tr('Your own skill')}
        subtitle={tr('Write instructions a bot can follow')}
        added={false}
        addLabel="New"
        icon={<TileIcon name="square.and.pencil" />}
        onAdd={onWriteSkill}
      />
      {loading ? <SkeletonGrid /> : null}
      {catalogued.length ? grouped(catalogued as (MarketSkill & { category: string; featured: boolean })[]).map((group) => (
        <MarketSection key={group.title} title={tr(group.title)}>
          <ItemGrid>
            {group.items.map((s) => (
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
        </MarketSection>
      )) : null}
      {other.length ? (
        <MarketSection title={tr('Anthropic skills')}>
          <ItemGrid>
            {other.map((s) => (
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
        </MarketSection>
      ) : null}
      {!loading && skills.length === 0 && query ? (
        <Text tone="secondary">{tr('No skills match “{query}”.').replaceAll('{query}', query)}</Text>
      ) : null}
    </div>
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
