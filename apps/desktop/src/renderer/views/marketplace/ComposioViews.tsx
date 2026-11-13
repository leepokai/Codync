import { useEffect, useRef, useState } from 'react'
import { Button, CardForm, CardSection, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage, useDismiss } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import {
  appendPage, appConnected, emptyPage, errorText, market, sleep,
  type ComposioApp, type ComposioConnect, type ComposioStatus, type MarketplacePage,
} from './market-models'
import {
  AppLogo, ErrorText, Field, FormSheet, ItemGrid, LoadMoreRow, MarketRow, MarketSection, SkeletonGrid, TileIcon, WebLink, filled,
} from './pieces'
import { usePlugins } from './plugins'
import { useTr } from '../../lib/use-language'
import { fill } from '../../lib/fill.ts'


/**
 * Apps through Composio (host/src/market/composio.rs): a key once, then connect apps with
 * Composio's own sign-in, which also works from the phone.
 */
export function ComposioSection({ query, onError }: { query: string; onError: (message: string) => void }) {
  const tr = useTr()
  const store = useStore()
  const plugins = usePlugins(store)
  const [status, setStatus] = useState<ComposioStatus | null>(null)
  const [page, setPage] = useState<MarketplacePage<ComposioApp>>(emptyPage<ComposioApp>)
  const [loading, setLoading] = useState(true)
  const [failed, setFailed] = useState<string | null>(null)
  const [settingUp, setSettingUp] = useState(false)
  const [connecting, setConnecting] = useState<ComposioApp | null>(null)
  const request = useRef(0)

  /** Answers for another search or page are late ones. */
  const load = async (search: string, cursor: string | null = null) => {
    const client = store.client
    if (!client) return
    const id = ++request.current
    setLoading(true)
    try {
      const s = await market.composioStatus(client)
      const more = s.configured ? await market.composioApps(client, search, cursor) : { items: [], next: null }
      if (request.current !== id) return
      setStatus(s)
      setPage((old) => (cursor ? appendPage(old, more.items, (a) => a.slug) : appendPage(emptyPage<ComposioApp>(), more.items, (a) => a.slug)))
      setFailed(null)
    } catch (e) {
      if (request.current === id) {
        setFailed(errorText(e))
        onError(errorText(e))
      }
    } finally {
      if (request.current === id) setLoading(false)
    }
  }

  useEffect(() => {
    const timer = window.setTimeout(() => void load(query), query ? 250 : 0)
    return () => window.clearTimeout(timer)
  }, [query])

  const configured = status?.configured === true
  return (
    <MarketSection title={tr('Composio')}>
      {!configured && !loading ? (
        <ItemGrid>
          <MarketRow
            title={tr('Composio')}
            subtitle={tr('Gmail, Slack, GitHub, Notion and 1,000 more apps')}
            added={false}
            busy={status === null}
            addLabel={tr('Set up')}
            icon={<TileIcon name="square.grid.3x3" />}
            onAdd={() => setSettingUp(true)}
          />
        </ItemGrid>
      ) : loading && page.items.length === 0 ? (
        <SkeletonGrid />
      ) : page.items.length === 0 ? (
        <Text tone="secondary">
          {failed ?? (query ? tr('No apps match “{query}”.').replaceAll('{query}', query) : tr(`Couldn't load apps.`))}
        </Text>
      ) : (
        <>
          <ItemGrid>
            {page.items.map((app) => (
              <MarketRow
                key={app.slug}
                title={app.name}
                subtitle={app.description ?? app.slug}
                added={appConnected(app)}
                addLabel={tr('Connect')}
                icon={<AppLogo url={app.logo} name={app.name} />}
                onAdd={() => setConnecting(app)}
              />
            ))}
          </ItemGrid>
          {page.next ? <LoadMoreRow cursor={page.next} failed={failed !== null} onLoad={() => void load(query, page.next)} /> : null}
        </>
      )}
      {configured ? (
        <button className="composio-settings" onClick={() => setSettingUp(true)}>
          <Icon name="key" size={10} />
          {tr('Composio settings')}
        </button>
      ) : null}
      <FormSheet open={settingUp} onClose={() => setSettingUp(false)}>
        <ComposioKeySheet
          status={status}
          onError={onError}
          done={(s) => {
            setStatus(s)
            setPage(emptyPage<ComposioApp>())
            void load(query)
          }}
        />
      </FormSheet>
      <FormSheet open={connecting !== null} onClose={() => setConnecting(null)}>
        {connecting ? (
          <ComposioConnectSheet
            key={connecting.slug}
            app={connecting}
            done={() => {
              void load(query)
              void plugins.refresh()
            }}
          />
        ) : null}
      </FormSheet>
    </MarketSection>
  )
}

/** Paste (or remove) the Composio API key; the host checks it with Composio before keeping it. */
export function ComposioKeySheet({ status, done, onError }: {
  status: ComposioStatus | null
  done: (status: ComposioStatus) => void
  onError: (message: string) => void
}) {
  const tr = useTr()
  const store = useStore()
  const dismiss = useDismiss()
  const [key, setKey] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const save = async (key: string) => {
    const client = store.client
    if (!client) return
    setSaving(true)
    try {
      done(await market.setComposioKey(client, key))
      dismiss()
    } catch (e) {
      setError(errorText(e))
      onError(errorText(e))
    }
    setSaving(false)
  }

  const keyUrl = status?.keyUrl ?? 'https://platform.composio.dev'
  return (
    <SheetPage>
      <ModalHeader
        icon="square.grid.3x3"
        title="Composio"
        trailing={saving ? <Spinner /> : <IconButton title={tr('Save')} icon="checkmark" disabled={!key.trim()} onClick={() => void save(key)} />}
      />
      <CardForm>
        <CardSection
          title={tr('Composio API key')}
          footer={tr('Composio signs you in to apps like Gmail and Slack and runs their tools for your bots. The key stays on {name}.').replaceAll('{name}', store.hostName)}
        >
          <Field
            label={tr('API key')}
            value={key}
            onChange={setKey}
            secret
            placeholder={status?.configured ? tr('Saved (paste to replace)') : 'ak_…'}
          />
          {URL.canParse(keyUrl) ? <WebLink title={tr('Get a key from Composio')} url={keyUrl} className="small" /> : null}
        </CardSection>
        {status?.configured ? (
          <CardSection footer={tr('Bots lose the apps you connected until you add a key again.')}>
            <button className="market-danger-button" onClick={() => void save('')}>{tr('Remove key')}</button>
          </CardSection>
        ) : null}
        {error ? <CardSection><ErrorText text={error} /></CardSection> : null}
      </CardForm>
    </SheetPage>
  )
}

/**
 * Connects one app: Composio's sign-in page (opened here, finished anywhere),
 * or a form for apps that use a key.
 */
export function ComposioConnectSheet({ app, requestId, done }: { app: ComposioApp; requestId?: string; done: () => void }) {
  const tr = useTr()
  const store = useStore()
  const dismiss = useDismiss()
  const [plan, setPlan] = useState<ComposioConnect | null>(null)
  const [values, setValues] = useState<Record<string, string>>({})
  const [connected, setConnected] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const connectedRef = useRef(false)
  const doneRef = useRef(done)
  doneRef.current = done

  const markConnected = () => {
    setValues({})
    connectedRef.current = true
    setConnected(true)
  }

  useEffect(() => {
    let cancelled = false
    void (async () => {
      const client = store.client
      if (!client) return
      try {
        const plan = await market.composioConnect(client, app.slug)
        if (cancelled) return
        setPlan(plan)
        if (plan.status !== 'redirect' || !plan.connection) return
        if (plan.url && URL.canParse(plan.url)) window.codync.app.openExternal(plan.url)
        // Polls until the sign-in finishes (or the sheet goes away).
        for (let i = 0; i < 300; i++) {
          await sleep(2000)
          if (cancelled) return
          const state = await market.composioConnection(client, plan.connection).catch(() => null)
          if (cancelled) return
          if (!state) continue
          if (state.status === 'active') {
            if (requestId) {
              try {
                await market.finishConnectionRequest(client, requestId, { connectorId: `composio-${app.slug}` })
              } catch (e) {
                if (!cancelled) setError(errorText(e))
                return
              }
            }
            if (!cancelled) markConnected()
            return
          }
          if (state.status === 'failed' || state.status === 'expired') {
            setError(fill(tr("Signing in to {name} didn't work ({status}). Close and try again."), { name: app.name, status: String(state.status) }))
            return
          }
        }
        setError(tr('Timed out waiting for the sign-in.'))
      } catch (e) {
        if (!cancelled) setError(errorText(e))
      }
    })()
    return () => {
      cancelled = true
      if (connectedRef.current) doneRef.current()
    }
  }, [store, app.slug, app.name, requestId])

  const submit = async () => {
    const client = store.client
    const mode = plan?.mode
    if (!client || !mode) return
    setSaving(true)
    try {
      const state = await market.composioConnectFields(client, app.slug, mode, filled(values))
      if (state.status === 'active') {
        if (requestId) await market.finishConnectionRequest(client, requestId, { connectorId: `composio-${app.slug}` })
        markConnected()
      } else {
        setError(fill(tr('{name} says the connection is {status}.'), { name: app.name, status: String(state.status) }))
      }
    } catch (e) {
      setError(errorText(e))
    }
    setSaving(false)
  }

  const needsFields = plan?.status === 'needsFields'
  const trailing = connected
    ? <IconButton title="Done" icon="checkmark" onClick={dismiss} />
    : needsFields
      ? saving
        ? <Spinner />
        : <IconButton title="Connect" icon="checkmark" disabled={plan?.fields?.some((f) => f.required && !values[f.name]) ?? true} onClick={() => void submit()} />
      : null

  const waitingRow = (text: string) => (
    <HStack gap={8}>
      <Spinner />
      <Text tone="secondary">{text}</Text>
    </HStack>
  )

  return (
    <SheetPage>
      <ModalHeader title={tr('Connect {name}').replaceAll('{name}', () => app.name)} trailing={trailing} />
      <CardForm>
        <CardSection>
          <HStack gap={14} className="market-card-header">
            <AppLogo url={app.logo} name={app.name} size={52} />
            <VStack gap={2} shrink>
              <Text size="title3" weight="semibold">{app.name}</Text>
              {app.description ? <Text size="subheadline" tone="secondary" lines={3}>{app.description}</Text> : null}
            </VStack>
          </HStack>
        </CardSection>
        {connected ? (
          <CardSection footer={tr("Turn {name} on for a bot in the bot's settings, under Connectors.").replaceAll('{name}', () => app.name)}>
            <HStack as="span" gap={6} className="composio-connected">
              <Icon name="checkmark.circle.fill" size={12} />
              Connected
            </HStack>
          </CardSection>
        ) : needsFields ? (
          <CardSection footer={tr('Sent to Composio, which keeps it for {name}.').replaceAll('{name}', () => app.name)}>
            {(plan?.fields ?? []).map((f) => (
              <VStack key={f.name} gap={4}>
                <Field label={f.label} value={values[f.name] ?? ''} secret={f.secret} placeholder={f.label} onChange={(v) => setValues((old) => ({ ...old, [f.name]: v }))} />
                {f.description ? <Text size="caption" tone="secondary">{f.description + (f.required ? '' : tr(` (optional)`))}</Text> : null}
              </VStack>
            ))}
          </CardSection>
        ) : plan?.status === 'redirect' ? (
          <CardSection footer="Sign in on the page that opened. This updates by itself when you're done.">
            {plan.url && URL.canParse(plan.url) ? (
              <div>
                <Button kind="secondary" onClick={() => window.codync.app.openExternal(plan.url!)}>
                  <Icon name="safari" size={11} />
                  Open the sign-in page
                </Button>
              </div>
            ) : null}
            {waitingRow('Waiting for you to finish signing in…')}
          </CardSection>
        ) : error === null ? (
          <CardSection>{waitingRow('Asking Composio…')}</CardSection>
        ) : null}
        {error ? <CardSection><ErrorText text={error} /></CardSection> : null}
      </CardForm>
    </SheetPage>
  )
}

/**
 * Paste (or remove) the Composio API key; the host checks it with Composio first.
 * The key stays on the computer.
 */
export function ComposioKeySheet({ status, done }: { status: ComposioStatus | null; done: (status: ComposioStatus) => void }) {
  const tr = useTr()
  const store = useStore()
  const dismiss = useDismiss()
  const [key, setKey] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const save = async (value: string) => {
    const client = store.client
    if (!client) return
    setSaving(true)
    setError(null)
    try {
      done(await market.setComposioKey(client, value.trim()))
      dismiss()
    } catch (e) {
      setError(errorText(e))
    }
    setSaving(false)
  }

  return (
    <SheetPage>
      <ModalHeader
        title="Composio"
        trailing={saving
          ? <Spinner />
          : <IconButton title="Save" icon="checkmark" disabled={!key.trim()} onClick={() => void save(key)} />}
      />
      <CardForm>
        <CardSection
          title={tr('Composio API key')}
          footer={tr(`Composio signs you in to apps like Gmail and Slack and runs their tools for your bots. The key stays on {name}.`).replaceAll('{name}', store.hostName)}
        >
          <Field label={tr(status?.configured ? 'Saved (paste to replace)' : 'API key')} value={key} secret placeholder="ak_…" onChange={setKey} />
          <WebLink url={status?.keyUrl || 'https://platform.composio.dev'} title={tr('Get a key from Composio')} className="small" />
        </CardSection>
        {status?.configured ? (
          <CardSection footer={tr('Bots lose the apps you connected until you add a key again.')}>
            <div>
              <Button kind="secondary" disabled={saving} onClick={() => void save('')}>{tr('Remove key')}</Button>
            </div>
          </CardSection>
        ) : null}
        {error ? <CardSection><ErrorText text={error} /></CardSection> : null}
      </CardForm>
    </SheetPage>
  )
}

