import { useEffect, useState } from 'react'
import type { N8nInstall, N8nState } from '@shared/models'
import { Button, CardForm, CardSection, Spinner, Switch, ValueRow } from '../../components/Controls'
import { FolderPicker } from '../../components/FolderPicker'
import { Sheet } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { lastPathComponent } from '../../lib/path'
import { useTr } from '../../lib/use-language'
import { StoreContext, useApp, useStore } from '../../store/context'
import { N8nWorkflows } from './N8nWorkflows'
import { errorText, thisMac } from './parts'

/** The n8n instance this computer runs: where it lives, and whether it's on. */
export function N8nSettings() {
  const app = useApp()
  const store = app.local
  const tr = useTr()
  if (!store) {
    return (
      <CardForm>
        <Text tone="secondary">{tr(`n8n is kept by ${thisMac}'s host, which isn't running.`)}</Text>
      </CardForm>
    )
  }
  return (
    <StoreContext.Provider value={store}>
      <N8nSettingsBody />
    </StoreContext.Provider>
  )
}

function N8nSettingsBody() {
  const store = useStore()
  const tr = useTr()

  // The store's copy comes from the events stream; the calls' answers cover the
  // settings that don't change the instance's state.
  const [live, setLive] = useState<N8nState | null>(store.n8n)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [picking, setPicking] = useState(false)
  const [port, setPort] = useState<string>(String(store.n8n?.port ?? 5679))

  useEffect(() => {
    setLive(store.n8n)
  }, [store.n8n])

  const online = store.connection.kind === 'online'

  const act = (method: string, body: object) =>
    store.ready().then(async (client) => {
      setBusy(true)
      setError(null)
      try {
        setLive(await client.call<N8nState>(method, body, 60_000))
      } catch (e) {
        setError(errorText(e))
      } finally {
        setBusy(false)
      }
    })

  const running = live?.state === 'running'
  const starting = live?.state === 'starting'
  const dot = live?.state === 'running' ? 'running' : live?.state === 'starting' ? 'starting' : live?.state === 'error' ? 'error' : 'off'

  return (
    <CardForm>
      <CardSection
        title="n8n"
        footer="An n8n instance run by this computer's host. Its editor opens in a window of Codync."
      >
        <Switch
          on={live?.enabled ?? false}
          onChange={(on) => void act('setN8nEnabled', { enabled: on })}
          disabled={busy || !online}
          label="n8n"
        >
          {tr('Run n8n with this computer')}
        </Switch>
        <div className="settings-row">
          <HStack gap={8} grow className="settings-stack">
            <span className={`n8n-dot ${dot}`} />
            <VStack grow>
              <Text truncate>{tr(stateLabel(live?.state ?? null))}</Text>
              {live?.error ? <Text size="caption" tone="danger" truncate>{live.error}</Text> : null}
            </VStack>
          </HStack>
          <HStack gap={6}>
            {running || starting ? (
              <Button kind="secondary" disabled={busy} onClick={() => void act('n8nStop', {})}>
                {tr('Stop')}
              </Button>
            ) : (
              <Button disabled={busy || !online || live === null} onClick={() => void act('n8nStart', {})}>
                {starting ? tr('Starting…') : tr('Start')}
              </Button>
            )}
            <Button kind="secondary" disabled={live === null} onClick={() => void window.codync.n8n.open()}>
              {tr('Open')}
            </Button>
          </HStack>
        </div>
        {error ? <Text size="footnote" tone="danger" className="fade-in">{error}</Text> : null}
      </CardSection>
      <CardSection title="Instance" footer={live?.url ?? null}>
        <ValueRow label="Install folder" detail={live?.path ? lastPathComponent(live.path) : tr('Downloaded by Codync')}>
          <HStack gap={6}>
            {live?.path ? (
              <Button kind="secondary" disabled={!online || busy} onClick={() => void act('setN8nPath', { path: '' })}>
                {tr('Use the one Codync downloads')}
              </Button>
            ) : null}
            <Button kind="secondary" disabled={!online} onClick={() => setPicking(true)}>
              {tr('Change…')}
            </Button>
          </HStack>
        </ValueRow>
        <ValueRow label="Port">
          <HStack gap={6}>
            <input
              className="port-field"
              type="number"
              min={1024}
              aria-label="Port"
              value={port}
              disabled={busy}
              onChange={(e) => setPort(e.target.value)}
            />
            <Button kind="secondary" disabled={busy || port === String(live?.port)} onClick={() => void act('setN8nPort', { port: Number(port) })}>
              {tr('Save')}
            </Button>
          </HStack>
        </ValueRow>
        {running ? (
          <ValueRow label="MCP server" detail={live?.mcp ? tr('Connected') : tr('Not connected')} />
        ) : null}
      </CardSection>
      {running ? <N8nWorkflows disabled={!online} /> : null}
      {starting ? (
        <CardSection>
          <HStack gap={8}>
            <Spinner size={14} />
            <Text tone="secondary">{tr(startingNote(live?.install ?? null))}</Text>
          </HStack>
          {live?.install?.phase === 'downloading' && live.install.total > 0 ? (
            <progress value={live.install.done} max={live.install.total} aria-label={tr('Downloading Node.js for n8n…')} />
          ) : null}
        </CardSection>
      ) : null}
      {live?.install?.phase === 'failed' && live.install.error ? (
        <CardSection>
          <Text size="footnote" tone="danger">{live.install.error}</Text>
        </CardSection>
      ) : null}
      <Sheet open={picking} onClose={() => setPicking(false)} width={460} height={520}>
        <FolderPicker
          start={live?.path ?? store.hello?.home ?? null}
          onPick={(path) => {
            setPicking(false)
            void act('setN8nPath', { path })
          }}
        />
      </Sheet>
    </CardForm>
  )
}

/** What the wait is for: the first start fetches Node.js and n8n, a later one only starts the instance. */
function startingNote(install: N8nInstall | null): string {
  switch (install?.phase) {
    case 'downloading': return 'Downloading Node.js for n8n…'
    case 'unpacking': return 'Unpacking Node.js…'
    case 'installing': return 'Installing n8n… the first time takes a few minutes.'
    default: return 'Starting… the first start can take a minute.'
  }
}

function stateLabel(state: string | null): string {
  switch (state) {
    case 'running': return 'Running'
    case 'starting': return 'Starting…'
    case 'error': return 'Not running'
    case 'stopped': return 'Stopped'
    default: return 'Unknown'
  }
}
