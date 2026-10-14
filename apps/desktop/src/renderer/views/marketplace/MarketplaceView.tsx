import { useEffect, useRef, useState, type ReactNode } from 'react'
import type { Backend } from '@shared/models'
import { Button, CardForm, CardSection, DropdownMenu, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog, useDismiss } from '../../components/Overlay'
import { font } from '../../lib/fonts'
import { useStore } from '../../store/context'
import { ComputerBadge } from '../ComputerBadge'
import { AgentSheet, AgentTile, NodeNotice } from './AgentSheet'
import { ComposioSection } from './ComposioViews'
import { CredentialsView } from './CredentialsView'
import { CustomConnectorSheet, InstallConnectorSheet, NewSkillSheet } from './ConnectorSheets'
import { SetupTerminalView } from './SetupTerminal'
import { SkillSheet, type SkillRef } from './SkillSheet'
import {
  appendPage, emptyPage, errorText, hasHiddenItems, market, needsSignIn, replacePage, revealMore, visible,
  type MarketConnector, type MarketSkill,
} from './market-models'
import { AppLogo, FormSheet, ItemGrid, MarketRow, MarketSection, ScreenHeader, ServiceLogo, SkeletonGrid, SkillGlyph, SlideStack, TileIcon } from './pieces'
import { usePlugins } from './plugins'

const connectorId = (c: MarketConnector) => c.name

/**
 * The Marketplace (Grok Bot's layout): one page of agents, connectors and
 * skills for the bots on one computer (the store in context). Installing happens on that
 * computer; each bot turns connectors and skills on in its settings.
 * With more than one computer, the header switches between them (the caller swaps the store).
 */
export function MarketplaceView({ computers, computer, onComputer, appsFirst = false }: { appsFirst?: boolean; computers: { id: string; name: string }[]; computer: string; onComputer: (id: string) => void }) {
  const store = useStore()
  const plugins = usePlugins(store)
  const dismiss = useDismiss()
  const [search, setSearch] = useState('')
  /** Bumped on each submitted search, so every section searches again. */
  const [searchToken, setSearchToken] = useState(0)
  const [connectors, setConnectors] = useState(emptyPage<MarketConnector>)
  /** The registry page after the ones shown; null at the end. */
  const [connectorCursor, setConnectorCursor] = useState<string | null>(null)
  const [loadingMore, setLoadingMore] = useState(false)
  const [skills, setSkills] = useState<MarketSkill[]>([])
  const [loadingConnectors, setLoadingConnectors] = useState(true)
  const [loadingSkills, setLoadingSkills] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [installing, setInstalling] = useState<MarketConnector | null>(null)
  const [busySkill, setBusySkill] = useState<string | null>(null)
  const [addingConnector, setAddingConnector] = useState(false)
  const [writingSkill, setWritingSkill] = useState(false)
  const [viewingSkill, setViewingSkill] = useState<SkillRef | null>(null)
  const [agent, setAgent] = useState<Backend | null>(null)
  const [addingNode, setAddingNode] = useState(false)
  /** Agents shown before "Load more agents" (two rows of the grid). */
  const [agentLimit, setAgentLimit] = useState(10)
  const [showInstalled, setShowInstalled] = useState(false)
  const [showCredentials, setShowCredentials] = useState(false)
  const request = useRef(0)
  const connectorQuery = useRef('')

  const query = search.trim()
  const matches = (text: string | null | undefined) => (text ?? '').toLowerCase().includes(query.toLowerCase())
  const allAgents = (store.hello?.backends ?? []).filter((b) => b.available || b.installed === true || b.curated === true)
  const agents = query ? allAgents.filter((b) => matches(b.name)) : allAgents
  const shownSkills = query ? skills.filter((s) => matches(s.name) || matches(s.description)) : skills
  const installedCount = plugins.installedConnectors.length + plugins.installedSkills.length

  const loadConnectors = async (q = query) => {
    const id = ++request.current
    connectorQuery.current = q
    setLoadingConnectors(true)
    setLoadingMore(false)
    try {
      const page = await market.marketConnectors(await store.ready(), q)
      if (request.current !== id) return
      setConnectors(replacePage(page.items, connectorId))
      setConnectorCursor(page.next)
      setError(null)
    } catch (e) {
      if (request.current !== id) return
      setConnectors(emptyPage())
      setConnectorCursor(null)
      setError(errorText(e))
    } finally {
      if (request.current === id) setLoadingConnectors(false)
    }
  }

  const loadMoreConnectors = async () => {
    if (loadingMore || loadingConnectors) return
    if (hasHiddenItems(connectors)) {
      setConnectors(revealMore)
      return
    }
    if (!connectorCursor) return
    const id = request.current
    setLoadingMore(true)
    try {
      const page = await market.marketConnectors(await store.ready(), connectorQuery.current, connectorCursor)
      if (request.current !== id) return
      setConnectors((old) => revealMore(appendPage(old, page.items, connectorId)))
      setConnectorCursor(page.next)
      setError(null)
    } catch (e) {
      if (request.current === id) setError(errorText(e))
    } finally {
      if (request.current === id) setLoadingMore(false)
    }
  }

  useEffect(() => {
    void plugins.refresh()
    // Picks up CLIs installed or signed in outside Codync.
    void store.refreshBackends()
    void loadConnectors('')
    void (async () => {
      try {
        setSkills(await market.marketSkills(await store.ready()))
      } catch (e) {
        setError(errorText(e))
      }
      setLoadingSkills(false)
    })()
    return () => void request.current++
    // Mount only, like the view's `.task`s.
  }, [])

  const submit = (q: string) => {
    setSearchToken((t) => t + 1)
    void loadConnectors(q)
  }

  const installSkill = async (s: MarketSkill) => {
    setBusySkill(s.source)
    try {
      await plugins.installSkill(s.source)
    } catch (e) {
      setError(errorText(e))
    }
    setBusySkill(null)
  }

  const header = (
    <div className="market-heading">
      {/* Everything here lives on one computer; say which. */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, minWidth: 0 }}>
        <ComputerBadge computer={store.computer} size={32} />
        <div style={{ display: 'flex', flexDirection: 'column', minWidth: 0, whiteSpace: 'nowrap' }}>
          <span style={{ ...font('title', 'semibold'), color: 'var(--text)' }}>Extensions</span>
          {computers.length > 1 ? (
            <DropdownMenu
              title="Switch computer"
              style={{ display: 'flex', alignItems: 'center', gap: 4, ...font('subheadline'), color: 'var(--secondary)', minWidth: 0 }}
              items={() => computers.map((c) => ({ title: c.name, selected: c.id === computer, action: () => onComputer(c.id) }))}
            >
              <span aria-label={`Computer: ${store.hostName}`} style={{ overflow: 'hidden', textOverflow: 'ellipsis' }}>{store.hostName}</span>
              <Icon name="chevron.down" size={10} weight="semibold" />
            </DropdownMenu>
          ) : (
            <span style={{ ...font('subheadline'), color: 'var(--secondary)', overflow: 'hidden', textOverflow: 'ellipsis' }}>{store.hostName}</span>
          )}
        </div>
      </div>
      <span style={{ flex: 1 }} />
      <Button kind="secondary" onClick={() => setShowCredentials(true)}><Icon name="lock.shield" size={12} />Credentials</Button>
      {installedCount > 0 ? (
        <button className="press" style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 'none' }} onClick={() => setShowInstalled(true)}>
          <span style={{ display: 'flex' }}>
            {plugins.installedConnectors.slice(0, 3).map((c, i) => (
              <span key={c.id} style={{ marginLeft: i ? -8 : 0, display: 'flex' }}>
                <ServiceLogo name={c.name} registryName={c.registryName} size={26} />
              </span>
            ))}
          </span>
          <span style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-start', color: 'var(--secondary)', whiteSpace: 'nowrap' }}>
            <span>{installedCount} installed</span>
            <span style={font('caption')}>Connectors & skills</span>
          </span>
          <Icon name="chevron.right" size={10} weight="semibold" color="var(--tertiary)" />
        </button>
      ) : null}
    </div>
  )

  const searchField = (
    <div className="market-search">
      <Icon name="magnifyingglass" size={13} color="var(--secondary)" />
      <input
        value={search}
        placeholder="Search agents, connectors and skills"
        aria-label="Search agents, connectors and skills"
        spellCheck={false}
        onChange={(e) => {
          setSearch(e.target.value)
          setAgentLimit(10)
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.nativeEvent.isComposing) submit(search.trim())
        }}
      />
      {search ? (
        <button
          aria-label="Clear search"
          title="Clear search"
          style={{ display: 'flex', color: 'var(--tertiary)' }}
          onClick={() => {
            setSearch('')
            setAgentLimit(10)
            submit('')
          }}
        >
          <Icon name="xmark.circle.fill" size={13} />
        </button>
      ) : null}
    </div>
  )

  const centered = (node: ReactNode) => <div style={{ display: 'flex', justifyContent: 'center' }}>{node}</div>

  const page = (
    <div className="market-page">
      <div className="market-scroll">
        <div className="market-page-content">
          {header}
          {searchField}
          {appsFirst ? <ComposioSection query={query} searchToken={searchToken} /> : null}
          {agents.length > 0 ? (
            <MarketSection title={query ? `Agents matching “${query}”` : 'Agents'}>
              {allAgents.some((b) => b.node === 'missing') ? <NodeNotice onAdd={() => setAddingNode(true)} /> : null}
              <div className="agent-grid">
                {agents.slice(0, agentLimit).map((b) => <AgentCard key={b.id} backend={b} onClick={() => setAgent(b)} />)}
              </div>
              {agents.length > agentLimit ? centered(<Button kind="secondary" onClick={() => setAgentLimit((n) => n + 10)}>Load more agents</Button>) : null}
            </MarketSection>
          ) : null}
          <MarketSection title="Connectors">
            {loadingConnectors ? (
              <SkeletonGrid />
            ) : connectors.items.length === 0 ? (
              <span style={{ color: 'var(--secondary)' }}>{query ? `No connectors match “${query}”.` : "Couldn't load connectors."}</span>
            ) : (
              <>
                <ItemGrid>
                  {visible(connectors).map((c) => (
                    <MarketRow
                      key={c.name}
                      title={c.title}
                      subtitle={c.description ?? c.name}
                      added={c.installed}
                      icon={<ServiceLogo website={c.website} name={c.title} registryName={c.name} />}
                      onAdd={() => setInstalling(c)}
                    />
                  ))}
                </ItemGrid>
                {hasHiddenItems(connectors) || connectorCursor
                  ? centered(
                      <Button kind="secondary" disabled={loadingMore} onClick={() => void loadMoreConnectors()}>
                        {loadingMore ? 'Loading…' : 'Load more connectors'}
                      </Button>,
                    )
                  : null}
              </>
            )}
          </MarketSection>
          {!appsFirst ? <ComposioSection query={query} searchToken={searchToken} /> : null}
          <MarketSection title="Skills">
            {loadingSkills ? (
              <SkeletonGrid />
            ) : (
              <ItemGrid>
                {shownSkills.map((s) => (
                  <MarketRow
                    key={s.source}
                    title={s.name}
                    subtitle={s.description}
                    added={s.installed || plugins.installedSkills.some((i) => i.id === s.source.toLowerCase())}
                    busy={busySkill === s.source}
                    icon={<SkillGlyph />}
                    onAdd={() => void installSkill(s)}
                    onOpen={() => setViewingSkill(s)}
                  />
                ))}
              </ItemGrid>
            )}
          </MarketSection>
          <MarketSection title="Make your own">
            <ItemGrid>
              <MarketRow
                title="Custom connector"
                subtitle="Any MCP server: a command or a URL"
                added={false}
                addLabel="New"
                icon={<TileIcon name="point.3.connected.trianglepath.dotted" />}
                onAdd={() => setAddingConnector(true)}
              />
              <MarketRow
                title="Your own skill"
                subtitle="Write instructions a bot can follow"
                added={false}
                addLabel="New"
                icon={<TileIcon name="square.and.pencil" />}
                onAdd={() => setWritingSkill(true)}
              />
            </ItemGrid>
          </MarketSection>
          {error ? <span style={{ ...font('footnote'), color: 'var(--danger)' }}>{error}</span> : null}
          <span style={{ ...font('footnote'), color: 'var(--tertiary)' }}>
            Connectors come from the official MCP Registry, apps through Composio, skills from Anthropic, agents from the ACP registry. Everything installs on {store.hostName}.
          </span>
        </div>
      </div>
      {/* Dismissing slides the sheet away; clearing the caller's state would cut it off. */}
      <div className="market-close">
        <IconButton title="Close" icon="xmark" onClick={dismiss} />
      </div>
    </div>
  )

  return (
    <>
      <SlideStack base={page} top={showCredentials ? <CredentialsView onBack={() => setShowCredentials(false)} /> : showInstalled ? <InstalledView back={() => setShowInstalled(false)} /> : null} />
      <FormSheet open={installing !== null} onClose={() => setInstalling(null)}>
        {installing ? <InstallConnectorSheet key={installing.name} item={installing} done={() => void loadConnectors(connectorQuery.current)} /> : null}
      </FormSheet>
      <FormSheet open={addingConnector} onClose={() => setAddingConnector(false)}>
        <CustomConnectorSheet />
      </FormSheet>
      <FormSheet open={writingSkill} onClose={() => setWritingSkill(false)}>
        <NewSkillSheet />
      </FormSheet>
      <FormSheet open={viewingSkill !== null} onClose={() => setViewingSkill(null)}>
        {viewingSkill ? <SkillSheet key={viewingSkill.name} skill={viewingSkill} /> : null}
      </FormSheet>
      <FormSheet open={addingNode} onClose={() => setAddingNode(false)} width={560} height={560}>
        {addingNode ? <SetupTerminalView backend={null} step="node" back={() => setAddingNode(false)} /> : null}
      </FormSheet>
      <FormSheet open={agent !== null} onClose={() => setAgent(null)} width={560} height={560}>
        {agent ? <AgentSheet key={agent.id} initial={agent} /> : null}
      </FormSheet>
    </>
  )
}

function AgentCard({ backend, onClick }: { backend: Backend; onClick: () => void }) {
  const status = backend.installed !== true && backend.curated === true ? 'Not installed'
    : backend.node === 'missing' ? 'Needs Node.js'
    : backend.installed !== true ? 'Sets up on first use'
    : backend.signedIn === false ? 'Not signed in'
    : 'Installed'
  return (
    <button className="agent-card" title={backend.name} onClick={onClick}>
      <AgentTile registry={backend.registry ?? backend.id} icon={24} tile={40} radius={11} />
      <span style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-start', gap: 3, minWidth: 0 }}>
        <span style={{ ...font('subheadline', 'semibold'), color: 'var(--text)', ...ellipsis }}>{backend.name}</span>
        <span style={{ ...font('caption'), color: 'var(--secondary)', ...ellipsis }}>{status}</span>
      </span>
    </button>
  )
}

const ellipsis = { whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', maxWidth: '100%' } as const

// MARK: - Installed

function InstalledView({ back }: { back: () => void }) {
  const store = useStore()
  const plugins = usePlugins(store)
  const [removing, setRemoving] = useState<{ id: string; name: string; isSkill: boolean } | null>(null)
  const [signingIn, setSigningIn] = useState<string | null>(null)
  const [viewingSkill, setViewingSkill] = useState<SkillRef | null>(null)
  const empty = plugins.installedConnectors.length === 0 && plugins.installedSkills.length === 0

  const signIn = async (id: string) => {
    setSigningIn(id)
    try {
      await plugins.signIn(id)
    } catch (e) {
      store.setError(errorText(e))
    }
    setSigningIn(null)
  }

  const remove = async (r: { id: string; isSkill: boolean }) => {
    try {
      if (r.isSkill) await plugins.removeSkill(r.id)
      else await plugins.removeConnector(r.id)
    } catch (e) {
      store.setError(errorText(e))
    }
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'var(--background)' }}>
      <ScreenHeader title="Connectors & skills" onBack={back} />
      <div style={{ flex: 1, minHeight: 0 }}>
        <CardForm>
          {empty ? <EmptyState icon="shippingbox" title="Nothing installed" detail="Connectors and skills you add show up here." /> : null}
          {plugins.installedConnectors.length ? (
            <CardSection title="Connectors">
              {plugins.installedConnectors.map((c) => (
                <InstalledRow
                  key={c.id}
                  title={c.name}
                  subtitle={needsSignIn(c) ? 'Sign in so bots can use it' : (c.command ?? c.url ?? c.description)}
                  icon={c.kind === 'composio' ? <AppLogo url={c.logo} name={c.name} size={32} /> : <ServiceLogo name={c.name} registryName={c.registryName} size={32} />}
                  accessory={signingIn === c.id ? <Spinner /> : needsSignIn(c) ? <Button onClick={() => void signIn(c.id)}>Sign in</Button> : null}
                  onRemove={() => setRemoving({ id: c.id, name: c.name, isSkill: false })}
                />
              ))}
            </CardSection>
          ) : null}
          {plugins.installedSkills.length ? (
            <CardSection title="Skills">
              {plugins.installedSkills.map((s) => (
                <InstalledRow
                  key={s.id}
                  title={s.name}
                  subtitle={s.description}
                  icon={<SkillGlyph size={32} />}
                  onOpen={() => setViewingSkill({ id: s.id, name: s.name, description: s.description })}
                  onRemove={() => setRemoving({ id: s.id, name: s.name, isSkill: true })}
                />
              ))}
            </CardSection>
          ) : null}
        </CardForm>
      </div>
      <Dialog
        open={removing !== null}
        title={`Remove ${removing?.name ?? ''}?`}
        message={removing ? (removing.isSkill ? 'Bots stop using this skill.' : 'Bots lose this connector, and the keys saved for it are deleted.') : null}
        actions={removing ? [{ title: 'Remove', destructive: true, action: () => void remove(removing) }] : []}
        onClose={() => setRemoving(null)}
      />
      <FormSheet open={viewingSkill !== null} onClose={() => setViewingSkill(null)}>
        {viewingSkill ? <SkillSheet key={viewingSkill.name} skill={viewingSkill} /> : null}
      </FormSheet>
    </div>
  )
}

/** A quiet "nothing here" message (replaces `ContentUnavailableView`). */
function EmptyState({ icon, title, detail }: { icon: string; title: string; detail: string }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 8, padding: '40px 0', textAlign: 'center' }}>
      <Icon name={icon} size={28} color="var(--tertiary)" />
      <span style={{ ...font('compactBody', 'semibold'), color: 'var(--text)' }}>{title}</span>
      <span style={{ ...font('compactSecondary'), color: 'var(--secondary)' }}>{detail}</span>
    </div>
  )
}

function InstalledRow({ title, subtitle, icon, accessory, onOpen, onRemove }: {
  title: string; subtitle: string; icon: ReactNode; accessory?: ReactNode
  /** Opens the item's details from its name. */
  onOpen?: () => void
  onRemove: () => void
}) {
  const label = (
    <>
      {icon}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 2, flex: 1, minWidth: 0, textAlign: 'left' }}>
        <span style={{ color: 'var(--text)', ...ellipsis }}>{title}</span>
        <span style={{ ...font('caption'), color: 'var(--secondary)', ...ellipsis }}>{subtitle}</span>
      </div>
    </>
  )
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 12 }} title={subtitle}>
      {onOpen ? (
        <button className="press" style={{ display: 'flex', alignItems: 'center', gap: 12, flex: 1, minWidth: 0 }} onClick={onOpen}>
          {label}
        </button>
      ) : label}
      <span style={{ minWidth: 12 }} />
      {accessory}
      <button
        title="Remove"
        aria-label={`Remove ${title}`}
        style={{ minWidth: 44, minHeight: 44, display: 'grid', placeItems: 'center', color: 'var(--secondary)', margin: '-8px 0' }}
        onClick={onRemove}
      >
        <Icon name="trash" size={13} />
      </button>
    </div>
  )
}
