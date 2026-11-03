import { useEffect, useState } from 'react'
import type { AutomanState } from '@shared/models'
import { Button, CardForm, CardSection, Spinner, Switch, ValueRow } from '../../components/Controls'
import { FolderPicker } from '../../components/FolderPicker'
import { Sheet } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { lastPathComponent } from '../../lib/path'
import { useTr } from '../../lib/use-language'
import { StoreContext, useApp, useStore } from '../../store/context'
import { errorText, thisMac } from './parts'

/** The Automations (n8n) instance this computer runs: where it lives, and whether it's on. */
export function AutomationsSettings() {
  const app = useApp()
  const store = app.local
  const tr = useTr()
  if (!store) {
    return (
      <CardForm>
        <Text tone="secondary">{tr(`Automations is kept by ${thisMac}'s host, which isn't running.`)}</Text>
      </CardForm>
    )
  }
  return (
    <StoreContext.Provider value={store}>
      <AutomationsSettingsBody />
    </StoreContext.Provider>
  )
}

function AutomationsSettingsBody() {
  const store = useStore()
  const tr = useTr()

  // The store's copy comes from the events stream; the calls' answers cover the
  // settings that don't change the instance's state.
  const [live, setLive] = useState<AutomanState | null>(store.automan)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [picking, setPicking] = useState(false)
  const [port, setPort] = useState<string>(String(store.automan?.port ?? 5679))

  useEffect(() => {
    setLive(store.automan)
  }, [store.automan])

  const online = store.connection.kind === 'online'

  const act = (method: string, body: object) =>
    store.ready().then(async (client) => {
      setBusy(true)
      setError(null)
      try {
        setLive(await client.call<AutomanState>(method, body, 60_000))
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
        title="Automations"
        footer="An n8n instance run by this computer's host. Its editor opens in a window of Codync."
      >
        <Switch
          on={live?.enabled ?? false}
          onChange={(on) => void act('setAutomanEnabled', { enabled: on })}
          disabled={busy || !online}
          label="Automations"
        >
          {tr('Run Automations with this computer')}
        </Switch>
        <div className="settings-row">
          <HStack gap={8} grow className="settings-stack">
            <span className={`automan-dot ${dot}`} />
            <VStack grow>
              <Text truncate>{tr(stateLabel(live?.state ?? null))}</Text>
              {live?.error ? <Text size="caption" tone="danger" truncate>{live.error}</Text> : null}
            </VStack>
          </HStack>
          <HStack gap={6}>
            {running || starting ? (
              <Button kind="secondary" disabled={busy} onClick={() => void act('automanStop', {})}>
                {tr('Stop')}
              </Button>
            ) : (
              <Button disabled={busy || !online || live === null} onClick={() => void act('automanStart', {})}>
                {starting ? tr('Starting…') : tr('Start')}
              </Button>
            )}
            <Button kind="secondary" disabled={live === null} onClick={() => void window.codync.automan.open()}>
              {tr('Open')}
            </Button>
          </HStack>
        </div>
        {error ? <Text size="footnote" tone="danger" className="fade-in">{error}</Text> : null}
      </CardSection>
      <CardSection title="Instance" footer={live?.url ?? tr('Set the install folder to choose where the instance runs.')}>
        <ValueRow label="Install folder" detail={live?.path ? lastPathComponent(live.path) : null}>
          <Button kind="secondary" disabled={!online} onClick={() => setPicking(true)}>
            {tr('Change…')}
          </Button>
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
            <Button kind="secondary" disabled={busy || port === String(live?.port)} onClick={() => void act('setAutomanPort', { port: Number(port) })}>
              {tr('Save')}
            </Button>
          </HStack>
        </ValueRow>
      </CardSection>
      {starting ? (
        <CardSection>
          <HStack gap={8}>
            <Spinner size={14} />
            <Text tone="secondary">{tr('Starting… the first start can take a minute.')}</Text>
          </HStack>
        </CardSection>
      ) : null}
      <Sheet open={picking} onClose={() => setPicking(false)} width={460} height={520}>
        <FolderPicker
          start={live?.path ?? store.hello?.home ?? null}
          onPick={(path) => {
            setPicking(false)
            void act('setAutomanPath', { path })
          }}
        />
      </Sheet>
    </CardForm>
  )
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
