import type { ReactNode } from 'react'
import type { Backend } from '@shared/models'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { AgentTile, NodeNotice } from './AgentSheet'
import type { ConnectorCatalog } from './connector-catalog'
import { clampPage, EXTENSION_SECTIONS, pageItems, type ExtensionSection, type MarketConnector, type MarketSkill } from './market-models'
import { ItemGrid, MarketRow, PageBar, ServiceLogo, SkeletonGrid, SkillGlyph, TileIcon } from './pieces'

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

interface Paging {
  page: number
  pageSize: number
  onPage: (page: number) => void
  onPageSize: (size: number) => void
}

export function AgentsPanel({ agents, query, needsNode, page, pageSize, onPage, onPageSize, onAddNode, onAgent }: Paging & {
  agents: Backend[]
  query: string
  needsNode: boolean
  onAddNode: () => void
  onAgent: (backend: Backend) => void
}) {
  const tr = useTr()
  const rows = pageItems(agents, page, pageSize)
  return (
    <>
      {needsNode ? <NodeNotice onAdd={onAddNode} /> : null}
      <div className="market-panel-scroll fade-in">
        {agents.length === 0 ? (
          <Text tone="secondary">{query ? tr('No agents match “{query}”.').replaceAll('{query}', query) : tr('No agents yet.')}</Text>
        ) : (
          <div className="agent-grid">
            {rows.map((b) => <AgentCard key={b.id} backend={b} onClick={() => onAgent(b)} />)}
            <PageSpacers count={pagePad(agents.length, rows.length, pageSize)} kind="agent" />
          </div>
        )}
        <PageBar page={page} count={agents.length} pageSize={pageSize} onPage={(next) => onPage(clampPage(next, agents.length, pageSize))} onPageSize={onPageSize} />
      </div>
    </>
  )
}

export function ConnectorsPanel({ catalog, query, pageSize, onPage, onPageSize, onAddConnector, onInstall }: Omit<Paging, 'page'> & {
  catalog: ConnectorCatalog<MarketConnector>
  query: string
  onAddConnector: () => void
  onInstall: (connector: MarketConnector) => void
}) {
  const tr = useTr()
  const all = catalog.items.items
  const rows = pageItems(all, catalog.page, pageSize)
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
        ) : all.length === 0 ? (
          <Text tone="secondary">{query ? tr('No connectors match “{query}”.').replaceAll('{query}', query) : tr(`Couldn't load connectors.`)}</Text>
        ) : rows.length === 0 ? (
          // The page past the loaded rows, while the registry fills it.
          <SkeletonGrid />
        ) : (
          <ItemGrid>
            {rows.map((c) => (
              <MarketRow
                key={c.name}
                title={c.title}
                subtitle={c.description ?? c.name}
                added={c.installed}
                icon={<ServiceLogo website={c.website} name={c.title} registryName={c.name} />}
                onAdd={() => onInstall(c)}
              />
            ))}
            <PageSpacers count={pagePad(all.length, rows.length, pageSize)} kind="row" />
          </ItemGrid>
        )}
        {catalog.loading ? null : (
          <PageBar page={catalog.page} count={all.length} pageSize={pageSize} hasMore={catalog.hasMore} loading={catalog.loadingMore} onPage={onPage} onPageSize={onPageSize} />
        )}
      </div>
    </>
  )
}

export function SkillsPanel({ skills, loading, query, installedSkillIds, busySkill, page, pageSize, onPage, onPageSize, onWriteSkill, onInstallSkill, onViewSkill }: Paging & {
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
  const rows = pageItems(skills, page, pageSize)
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
            {rows.map((s) => (
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
            <PageSpacers count={pagePad(skills.length, rows.length, pageSize)} kind="row" />
          </ItemGrid>
        )}
        {loading ? null : (
          <PageBar page={page} count={skills.length} pageSize={pageSize} onPage={(next) => onPage(clampPage(next, skills.length, pageSize))} onPageSize={onPageSize} />
        )}
      </div>
    </>
  )
}

function pagePad(count: number, shown: number, size: number) {
  return count > size ? Math.max(0, size - shown) : 0
}

/** Rooms the size of a row, with nothing in them: a short last page keeps a full page's height, so the bar stays put. */
function PageSpacers({ count, kind }: { count: number; kind: 'agent' | 'row' }) {
  const cells: ReactNode[] = []
  for (let i = 0; i < count; i++) {
    cells.push(kind === 'agent' ? (
      <div key={i} className="agent-card page-spacer" aria-hidden="true">
        <AgentCardBody registry={null} name={'\u00a0'} status={'\u00a0'} />
      </div>
    ) : (
      <div key={i} className="page-spacer" aria-hidden="true">
        <MarketRow title={'\u00a0'} subtitle={'\u00a0'} added icon={null} onAdd={() => {}} />
      </div>
    ))
  }
  return <>{cells}</>
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
      <AgentCardBody registry={backend.registry ?? backend.id} name={backend.name} status={tr(status)} />
    </button>
  )
}

/** An agent tile's logo, name and status line. */
function AgentCardBody({ registry, name, status }: { registry: string | null; name: string; status: string }) {
  return (
    <>
      <AgentTile registry={registry} icon={24} tile={40} radius={11} />
      <VStack as="span" align="flex-start" gap={3} shrink>
        <Text size="subheadline" weight="semibold" tone="text" truncate className="agent-card-line">{name}</Text>
        <Text size="caption" tone="secondary" truncate className="agent-card-line">{status}</Text>
      </VStack>
    </>
  )
}
