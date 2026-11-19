import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { devServerAddress, devServerAddressTitle, devServerClick, devServerMenuRows, devServerValue, type DevServer, type DevServerMenuRow, type DevServerSummary } from '@shared/dev-servers'
import type { Bot } from '@shared/models'
import { Button, IconButton, Spinner } from '../../components/Controls'
import { ModalHeader, Sheet, SheetPage } from '../../components/Overlay'
import { AnchoredMenu, type MenuItem } from '../../components/Menu'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { analytics } from '../../store/analytics'
import type { BotStore } from '../../store/bot-store'
import { useApp, useStore } from '../../store/context'
import './dev-servers.css'

/** A bot's dev servers in the title: the port in quiet text, or nothing. */
export function DevServerTitle({ summary }: { summary: DevServerSummary }) {
  const tr = useTr()
  switch (summary.kind) {
    case 'none':
      return null
    case 'running':
      return <Text size="body" tone="secondary" tabular className="title-pill-port">{summary.text}</Text>
    case 'starting':
      return <><Spinner size={11} /><Text size="body" tone="secondary" className="title-pill-port">{tr(`Starting`)}</Text></>
    case 'failed':
      return <Text size="body" tone="danger" className="title-pill-port">{tr(`Failed to start`)}</Text>
  }
}

/** What a bot keeps running: a row is its name and port, opening it is a click, the rest is behind `⋯`. */
export function DevServersSection({ bot }: { bot: Bot }) {
  const tr = useTr()
  const [logs, setLogs] = useState<DevServer | null>(null)
  if (bot.devServers.length === 0) return null
  return (
    <section className="details-section">
      <Text className="details-section-title">{tr(`Dev servers`)}</Text>
      <VStack className="outlined-card">
        {bot.devServers.map((server) => <Row key={server.id} bot={bot} server={server} showLogs={() => setLogs(server)} />)}
      </VStack>
      <Sheet open={logs !== null} onClose={() => setLogs(null)} width={620} dismissOnDim>
        {logs ? <Logs bot={bot} server={logs} /> : null}
      </Sheet>
    </section>
  )
}

function Row({ bot, server, showLogs }: { bot: Bot; server: DevServer; showLogs: () => void }) {
  const tr = useTr()
  const store = useStore()
  const app = useApp()
  const [menu, setMenu] = useState(false)
  const more = useRef<HTMLButtonElement>(null)
  // Not `store.isLoopback`: a store attached over SSH is loopback too, through its tunnel.
  const onThisComputer = store === app.local
  const clicked = () => {
    const click = devServerClick(server, onThisComputer)
    switch (click.kind) {
      case 'open': return openAddress(click.address)
      case 'logs': return showLogs()
      case 'menu': return setMenu(true)
    }
  }
  const items = () => menuItems(bot, server, onThisComputer, store, showLogs)
  return (
    <HStack className="dev-server-row">
      <button className="dev-server-open" title={tooltip(server, tr)} onClick={clicked}>
        <Text grow truncate className="dev-server-name">{server.name}</Text>
        {server.status === 'starting' ? <Spinner size={11} /> : null}
        <Text tabular tone={server.status === 'failed' ? 'danger' : 'secondary'}>{tr(devServerValue(server))}</Text>
      </button>
      <IconButton
        className="dev-server-more"
        title={tr('More actions for {name}').replaceAll('{name}', () => server.name)}
        icon="ellipsis"
        size={24}
        buttonRef={more}
        menuOpen={menu}
        onClick={() => setMenu(true)}
      />
      <AnchoredMenu open={menu} onClose={() => setMenu(false)} anchor={more} items={items} />
    </HStack>
  )
}

function tooltip(server: DevServer, tr: (text: string) => string): string {
  const code = server.exitCode !== null && server.status === 'failed' ? ` · ${tr('Exit code {code}').replaceAll('{code}', String(server.exitCode))}` : ''
  return `${server.command}${code}`
}

function openAddress(address: string) {
  analytics.track({ event: 'dev_server_action', action: 'open' })
  window.codync.app.openExternal(address)
}

function menuItems(bot: Bot, server: DevServer, onThisComputer: boolean, store: BotStore, showLogs: () => void): MenuItem[] {
  const address = devServerAddress(server, onThisComputer)
  const copyAddress = (local: string) => {
    analytics.track({ event: 'dev_server_action', action: 'copy' })
    window.codync.app.copy(address ?? local)
  }
  const item = (row: DevServerMenuRow, divider: boolean): MenuItem => {
    if (typeof row === 'object') return { title: devServerAddressTitle(row.open), icon: 'arrow.up.right', verbatim: true, action: () => openAddress(row.open) }
    switch (row) {
      case 'copyAddress':
        // Elsewhere, a server that listens on its computer only can't be opened from here.
        return { title: 'Copy address', icon: 'doc.on.doc', detail: address ? undefined : 'Only reachable on the computer', action: () => copyAddress(server.ports[0].local) }
      case 'logs':
        return { title: 'Logs', icon: 'doc.text', action: showLogs }
      case 'restart':
        return { title: 'Restart', icon: 'arrow.clockwise', action: () => store.devServers.act('devServerRestart', bot.id, server.id) }
      case 'start':
        return { title: 'Start', icon: 'arrow.clockwise', action: () => store.devServers.act('devServerRestart', bot.id, server.id) }
      case 'stop':
        return {
          title: 'Stop',
          icon: 'stop.fill',
          destructive: true,
          divider,
          detail: server.managed ? undefined : "Started through the bot's shell: it stops with the bot",
          action: () => store.devServers.act('devServerStop', bot.id, server.id),
        }
      case 'remove':
        return { title: 'Remove', icon: 'trash', destructive: true, divider, action: () => store.devServers.act('devServerRemove', bot.id, server.id) }
    }
  }
  return devServerMenuRows(server, onThisComputer).map((row, index) => item(row, index > 0))
}

function Logs({ bot, server }: { bot: Bot; server: DevServer }) {
  const tr = useTr()
  const store = useStore()
  const [text, setText] = useState<string | null>(null)
  const [failed, setFailed] = useState<string | null>(null)
  const load = useCallback(() => {
    setFailed(null)
    store.devServers.logs(bot.id, server.id).then(setText, (error: unknown) => setFailed(error instanceof Error ? error.message : String(error)))
  }, [store, bot.id, server.id])
  useEffect(load, [load])
  const state = (content: ReactNode) => <VStack gap={12} align="center" justify="center" className="dev-server-logs-state">{content}</VStack>
  return (
    <SheetPage>
      <ModalHeader
        title={server.name}
        subtitle={<Text mono size="caption" tone="secondary">{tooltip(server, tr)}</Text>}
        trailing={<IconButton title="Refresh" icon="arrow.clockwise" onClick={load} />}
      />
      {failed ? (
        state(<><Text tone="danger">{failed}</Text><Button kind="secondary" onClick={load}>Try again</Button></>)
      ) : text === null ? (
        state(<Spinner />)
      ) : text === '' ? (
        state(<Text tone="secondary">{tr(`No output yet`)}</Text>)
      ) : (
        <pre className="dev-server-logs selectable">{text}</pre>
      )}
    </SheetPage>
  )
}
