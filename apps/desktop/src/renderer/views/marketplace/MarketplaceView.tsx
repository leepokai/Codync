import { useEffect, useState } from 'react'
import type { Backend } from '@shared/models'
import { Button, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog, SheetPage, useDismiss } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { useScreenViewed } from '../../store/analytics'
import { ComputerBadge } from '../ComputerBadge'
import { AgentSheet } from './AgentSheet'
import { CredentialsView } from './CredentialsView'
import { CustomConnectorSheet, InstallConnectorSheet, NewSkillSheet } from './ConnectorSheets'
import { SetupTerminalView } from './SetupTerminal'
import { SkillSheet, type SkillRef } from './SkillSheet'
import { useModel } from '../../lib/observable'
import { ConnectorCatalog } from './connector-catalog'
import { errorText, market, needsSignIn, type ExtensionSection, type MarketConnector, type MarketSkill } from './market-models'
import { AgentsPanel, ConnectorsPanel, MarketPanel, SkillsPanel } from './MarketplacePanel'
import { AppLogo, ExtensionTabs, FormSheet, ItemGrid, ItemRow, MarketSection, ScreenHeader, ServiceLogo, SkillGlyph, SlideStack } from './pieces'
import { usePlugins } from './plugins'
import { useTr } from '../../lib/use-language'

/**
 * Extensions for the bots on one computer (the store in context): Agents, Connectors
 * and Skills, one at a time. Each list scrolls; the connectors load more from the registry as
 * their end comes into view (`ConnectorCatalog`). Installing happens on that computer; each bot
 * turns connectors and skills on in its settings. Each computer has its own, opened
 * from its heading in the sidebar.
 */
export function MarketplaceView() {
  const store = useStore()
  const tr = useTr()
  const plugins = usePlugins(store)
  const dismiss = useDismiss()
  useScreenViewed('marketplace')
  const [search, setSearch] = useState('')
  const [catalog] = useState(() => new ConnectorCatalog<MarketConnector>(
    (c) => c.name,
    async (q, cursor) => market.marketConnectors(await store.ready(), q, cursor),
  ))
  const connectors = useModel(catalog)
  const [skills, setSkills] = useState<MarketSkill[]>([])
  const [loadingSkills, setLoadingSkills] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [installing, setInstalling] = useState<MarketConnector | null>(null)
  const [viewingConnector, setViewingConnector] = useState<MarketConnector | null>(null)
  const [busySkill, setBusySkill] = useState<string | null>(null)
  const [addingConnector, setAddingConnector] = useState(false)
  const [writingSkill, setWritingSkill] = useState(false)
  const [viewingSkill, setViewingSkill] = useState<SkillRef | null>(null)
  const [agent, setAgent] = useState<Backend | null>(null)
  const [addingNode, setAddingNode] = useState(false)
  const [section, setSection] = useState<ExtensionSection>('agents')
  const [showInstalled, setShowInstalled] = useState(false)
  const [showCredentials, setShowCredentials] = useState(false)

  const query = search.trim()
  const matches = (text: string | null | undefined) => (text ?? '').toLowerCase().includes(query.toLowerCase())
  const allAgents = (store.hello?.backends ?? []).filter((b) => b.available || b.installed === true || b.curated === true)
  const agents = query ? allAgents.filter((b) => matches(b.name)) : allAgents
  const shownSkills = query ? skills.filter((s) => matches(s.name) || matches(s.description)) : skills
  const installedCount = plugins.installedConnectors.length + plugins.installedSkills.length

  useEffect(() => {
    void plugins.refresh()
    // Picks up CLIs installed or signed in outside Codync.
    void store.refreshBackends()
    void connectors.load('')
    void (async () => {
      try {
        setSkills(await market.marketSkills(await store.ready()))
      } catch (e) {
        setError(errorText(e))
      }
      setLoadingSkills(false)
    })()
    return () => connectors.cancel()
    // Mount only, like the view's `.task`s.
  }, [])

  const submit = (q: string) => {
    void connectors.load(q)
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
      <HStack gap={10} shrink>
        <ComputerBadge computer={store.computer} size={32} />
        <VStack shrink className="market-heading-titles">
          <Text size="title" weight="semibold" tone="text">{tr(`Extensions`)}</Text>
          <Text size="subheadline" tone="secondary" truncate>{store.hostName}</Text>
        </VStack>
      </HStack>
      <Spacer />
      <Button kind="secondary" onClick={() => setShowCredentials(true)}><Icon name="lock.shield" size={12} />{tr('Credentials')}</Button>
      {installedCount > 0 ? (
        <button className="press market-installed" onClick={() => setShowInstalled(true)}>
          <HStack as="span" align="stretch" className="market-installed-logos">
            {plugins.installedConnectors.slice(0, 3).map((c) => (
              <HStack as="span" align="stretch" key={c.id}>
                <ServiceLogo name={c.name} registryName={c.registryName} size={26} />
              </HStack>
            ))}
          </HStack>
          <VStack as="span" align="flex-start" className="market-installed-text">
            <Text tone="secondary">{tr('{count} installed').replaceAll('{count}', String(installedCount))}</Text>
            <Text size="caption" tone="secondary">{tr(`Connectors & skills`)}</Text>
          </VStack>
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
        placeholder={tr('Search agents, connectors and skills')}
        aria-label={tr('Search agents, connectors and skills')}
        spellCheck={false}
        onChange={(e) => setSearch(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.nativeEvent.isComposing) submit(search.trim())
        }}
      />
      {search ? (
        <button
          aria-label={tr('Clear search')}
          title={tr('Clear search')}
          className="market-search-clear"
          onClick={() => {
            setSearch('')
            submit('')
          }}
        >
          <Icon name="xmark.circle.fill" size={13} />
        </button>
      ) : null}
    </div>
  )

  const page = (
    <div className="market-page">
      <div className="market-frame">
        {header}
        {searchField}
        <ExtensionTabs selection={section} onChange={setSection} />
        <MarketPanel section={section} hostName={store.hostName} error={error ?? connectors.error}>
          {section === 'agents' ? (
            <AgentsPanel
              agents={agents}
              query={query}
              needsNode={allAgents.some((b) => b.node === 'missing')}
              onAddNode={() => setAddingNode(true)}
              onAgent={setAgent}
            />
          ) : section === 'connectors' ? (
            <ConnectorsPanel
              catalog={connectors}
              query={query}
              onAddConnector={() => setAddingConnector(true)}
              onInstall={setInstalling}
              onView={setViewingConnector}
            />
          ) : (
            <SkillsPanel
              skills={shownSkills}
              loading={loadingSkills}
              query={query}
              installedSkillIds={plugins.installedSkills.map((s) => s.id)}
              busySkill={busySkill}
              onWriteSkill={() => setWritingSkill(true)}
              onInstallSkill={(s) => void installSkill(s)}
              onViewSkill={setViewingSkill}
            />
          )}
        </MarketPanel>
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
        {installing ? <InstallConnectorSheet key={installing.name} item={installing} done={() => void connectors.reload()} /> : null}
      </FormSheet>
      <FormSheet open={viewingConnector !== null} onClose={() => setViewingConnector(null)}>
        {viewingConnector ? <InstallConnectorSheet key={viewingConnector.name} item={viewingConnector} addsAtOnce={false} done={() => void connectors.reload()} /> : null}
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
      <FormSheet open={addingNode} onClose={() => setAddingNode(false)} width={560} height={560} fixed>
        {addingNode ? <SetupTerminalView backend={null} step="node" back={() => setAddingNode(false)} /> : null}
      </FormSheet>
      <FormSheet open={agent !== null} onClose={() => setAgent(null)} width={560} height={560} fixed>
        {agent ? <AgentSheet key={agent.id} initial={agent} /> : null}
      </FormSheet>
    </>
  )
}

// MARK: - Installed

function InstalledView({ back }: { back: () => void }) {
  const store = useStore()
  const tr = useTr()
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
    <SheetPage>
      <ScreenHeader title="Connectors & skills" onBack={back} />
      <div className="market-installed-body">
        <div className="market-scroll">
          <div className="market-page-content">
            {empty ? <EmptyState icon="shippingbox" title="Nothing installed" detail="Connectors and skills you add show up here." /> : null}
            {plugins.installedConnectors.length ? (
              <MarketSection title="Connectors">
                <ItemGrid>
                  {plugins.installedConnectors.map((c) => (
                    <ItemRow
                      key={c.id}
                      title={c.name}
                      subtitle={needsSignIn(c) ? 'Sign in so bots can use it' : (c.command ?? c.url ?? c.description)}
                      icon={c.kind === 'composio' ? <AppLogo url={c.logo} name={c.name} /> : <ServiceLogo name={c.name} registryName={c.registryName} />}
                      trailing={(
                        <>
                          {signingIn === c.id ? <Spinner /> : needsSignIn(c) ? <Button onClick={() => void signIn(c.id)}>Sign in</Button> : null}
                          <RemoveButton title={c.name} onClick={() => setRemoving({ id: c.id, name: c.name, isSkill: false })} />
                        </>
                      )}
                    />
                  ))}
                </ItemGrid>
              </MarketSection>
            ) : null}
            {plugins.installedSkills.length ? (
              <MarketSection title="Skills">
                <ItemGrid>
                  {plugins.installedSkills.map((s) => (
                    <ItemRow
                      key={s.id}
                      title={s.name}
                      subtitle={s.description}
                      icon={<SkillGlyph />}
                      onOpen={() => setViewingSkill({ id: s.id, name: s.name, description: s.description })}
                      trailing={<RemoveButton title={s.name} onClick={() => setRemoving({ id: s.id, name: s.name, isSkill: true })} />}
                    />
                  ))}
                </ItemGrid>
              </MarketSection>
            ) : null}
          </div>
        </div>
      </div>
      <Dialog
        open={removing !== null}
        title={tr('Remove {name}?').replaceAll('{name}', removing?.name ?? '')}
        message={removing ? tr(removing.isSkill ? 'Bots stop using this skill.' : 'Bots lose this connector, and the keys saved for it are deleted.') : null}
        actions={removing ? [{ title: 'Remove', destructive: true, action: () => void remove(removing) }] : []}
        onClose={() => setRemoving(null)}
      />
      <FormSheet open={viewingSkill !== null} onClose={() => setViewingSkill(null)}>
        {viewingSkill ? <SkillSheet key={viewingSkill.name} skill={viewingSkill} /> : null}
      </FormSheet>
    </SheetPage>
  )
}

/** A quiet "nothing here" message (replaces `ContentUnavailableView`). */
function EmptyState({ icon, title, detail }: { icon: string; title: string; detail: string }) {
  const tr = useTr()
  return (
    <VStack align="center" gap={8} className="market-empty">
      <Icon name={icon} size={28} color="var(--tertiary)" />
      <Text size="compactBody" weight="semibold" tone="text">{tr(title)}</Text>
      <Text size="compactSecondary" tone="secondary">{tr(detail)}</Text>
    </VStack>
  )
}

function RemoveButton({ title, onClick }: { title: string; onClick: () => void }) {
  return (
    <button
      title="Remove"
      aria-label={`Remove ${title}`}
      className="market-remove"
      onClick={(e) => {
        e.stopPropagation()
        onClick()
      }}
    >
      <Icon name="trash" size={13} />
    </button>
  )
}
