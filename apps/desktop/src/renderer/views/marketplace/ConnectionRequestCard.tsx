import { useState } from 'react'
import type { Backend, Entry } from '@shared/models'
import { Button, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { AgentSheet, AgentTile } from './AgentSheet'
import { ComposioConnectSheet } from './ComposioViews'
import { InstallConnectorSheet } from './ConnectorSheets'
import {
  errorText, market, needsSignIn,
  type ComposioApp, type ConnectionRequest, type MarketConnector,
} from './market-models'
import { AppLogo, Field, FormSheet, ServiceLogo } from './pieces'
import { pluginsFor } from './plugins'
import { useTr } from '../../lib/use-language'


/** Credentials go through the setup API; only the request's state is a chat entry. */
export function ConnectionRequestCard({ entry }: { entry: Entry }) {
  const tr = useTr()
  const store = useStore()
  const request = entry.data.connectionRequest as unknown as ConnectionRequest
  const [app, setApp] = useState<ComposioApp | null>(null)
  const [item, setItem] = useState<MarketConnector | null>(null)
  const [agent, setAgent] = useState<Backend | null>(null)
  const [value, setValue] = useState('')
  const [username, setUsername] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const isLogin = request.kind === 'login'
  const isSecret = request.kind === 'secret'
  const isAgent = request.kind === 'agent'

  const connect = async () => {
    const client = store.client
    if (!client) return
    setBusy(true)
    setError(null)
    try {
      if (isAgent) {
        // The host completes the card and resends the messages once the sheet finds it signed in.
        const backend = store.hello?.backends.find((b) => b.id === request.backend)
        if (backend) setAgent(backend)
        else setError(`Open ${request.title} in Extensions to sign in.`)
      } else if (request.kind === 'app' && request.toolkit) {
        setApp({ slug: request.toolkit, name: request.title })
      } else if (isLogin) {
        await market.finishConnectionRequest(client, entry.id, { value, username })
        setValue('')
      } else if (isSecret) {
        await market.finishConnectionRequest(client, entry.id, { value })
        setValue('')
      } else if (request.connectorId) {
        const id = request.connectorId
        const connector = (await market.connectors(client)).find((c) => c.id === id)
        if (connector && needsSignIn(connector)) await pluginsFor(store).signIn(id)
        await market.finishConnectionRequest(client, entry.id, { connectorId: id })
      } else if (request.registryName) {
        setItem(await market.connectorInfo(client, request.registryName))
      }
    } catch (e) {
      setError(errorText(e))
    }
    setBusy(false)
  }

  const pending = request.status === 'pending'
  return (
    <VStack gap={12} className="request-card">
      <HStack gap={14}>
        <RequestLogo request={request} />
        <VStack gap={2} grow>
          <Text size="body" weight="semibold" tone="text" truncate>{request.title}</Text>
          {entry.data.text ? <Text size="subheadline" tone="secondary" lines={2} title={entry.data.text}>{entry.data.text}</Text> : null}
        </VStack>
        {pending ? (
          <HStack gap={6} className="request-card-trailing">
            {busy ? <Spinner /> : null}
            <Button disabled={busy || ((isSecret || isLogin) && !value)} onClick={() => void connect()}>
              {isAgent ? 'Sign in' : isLogin ? 'Save' : 'Connect'}
            </Button>
          </HStack>
        ) : (
          <HStack as="span" gap={6} className="request-card-trailing request-card-status">
            {request.status === 'ready' ? <Icon name="checkmark" size={11} weight="semibold" /> : null}
            {request.status === 'ready' ? (isAgent ? 'Signed in' : isLogin ? 'Saved' : 'Connected') : 'Cancelled'}
          </HStack>
        )}
      </HStack>
      {pending && isLogin ? (
        <>
          <Field value={username} onChange={setUsername} placeholder="Username or email" />
          <Field value={value} onChange={setValue} placeholder="Password or op:// reference" secret />
          <Text size="caption" tone="secondary">{tr(`Saved securely on this computer. The bot types it into the sign-in page but never sees it.`)}</Text>
        </>
      ) : pending && isSecret ? (
        <>
          <Text size="caption" tone="secondary">{`${request.field ?? tr(`Credential`)} · ${request.location ?? ''}`}</Text>
          <Field value={value} onChange={setValue} placeholder="Credential or op:// reference" secret />
          <Text size="caption" tone="secondary">{tr(`Saved securely on this computer. Never added to the conversation.`)}</Text>
        </>
      ) : null}
      {error ? <Text size="footnote" tone="danger">{error}</Text> : null}
      <FormSheet open={item !== null} onClose={() => setItem(null)}>
        {item ? <InstallConnectorSheet item={item} requestId={entry.id} done={() => setItem(null)} /> : null}
      </FormSheet>
      <FormSheet open={app !== null} onClose={() => setApp(null)}>
        {app ? <ComposioConnectSheet app={app} requestId={entry.id} done={() => setApp(null)} /> : null}
      </FormSheet>
      <FormSheet open={agent !== null} onClose={() => setAgent(null)} width={560} height={560} fixed>
        {agent ? <AgentSheet key={agent.id} initial={agent} recheck /> : null}
      </FormSheet>
    </VStack>
  )
}

/** The agent, website or service a request is for. */
function RequestLogo({ request }: { request: ConnectionRequest }) {
  const store = useStore()
  if (request.kind === 'agent') {
    const backend = store.hello?.backends.find((b) => b.id === request.backend)
    return <AgentTile registry={backend?.registry ?? request.backend} icon={26} tile={46} radius={12} />
  }
  if (request.kind === 'app') return <AppLogo url={request.logo} name={request.title} />
  // A login's site is a domain, or an app's name (no logo to find for that).
  if (request.kind === 'login') return <ServiceLogo site={request.site?.includes('.') ? request.site : null} name={request.title} />
  return <ServiceLogo name={request.title} registryName={request.registryName} />
}
