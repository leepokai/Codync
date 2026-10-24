import { useEffect, useRef, useState } from 'react'
import { Button, CardForm, CardSection, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage, useDismiss } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import {
  errorText, market, sleep,
  type ComposioApp, type ComposioConnect,
} from './market-models'
import { AppLogo, ErrorText, Field, filled } from './pieces'
import { useTr } from '../../lib/use-language'


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
            setError(`Signing in to ${app.name} didn't work (${state.status}). Close and try again.`)
            return
          }
        }
        setError('Timed out waiting for the sign-in.')
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
        setError(`${app.name} says the connection is ${state.status}.`)
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
      <ModalHeader title={`Connect ${app.name}`} trailing={trailing} />
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
          <CardSection footer={`Turn ${app.name} on for a bot in the bot's settings, under Connectors.`}>
            <HStack as="span" gap={6} className="composio-connected">
              <Icon name="checkmark.circle.fill" size={12} />
              Connected
            </HStack>
          </CardSection>
        ) : needsFields ? (
          <CardSection footer={`Sent to Composio, which keeps it for ${app.name}.`}>
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
