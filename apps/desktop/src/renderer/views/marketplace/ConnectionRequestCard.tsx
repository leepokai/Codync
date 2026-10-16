import { useState } from 'react'
import type { Backend, Entry } from '@shared/models'
import { Button, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { font } from '../../lib/fonts'
import { useStore } from '../../store/context'
import { AgentSheet, AgentTile } from './AgentSheet'
import { ComposioConnectSheet } from './ComposioViews'
import { InstallConnectorSheet } from './ConnectorSheets'
import {
  errorText, market, needsSignIn,
  type ComposioApp, type ConnectionRequest, type MarketConnector,
} from './market-models'
import { AppLogo, Field, FormSheet, ServiceLogo, lineClamp } from './pieces'
import { pluginsFor } from './plugins'

const caption = { ...font('caption'), color: 'var(--secondary)' }

/** Credentials go through the setup API; only the request's state is a chat entry. */
export function ConnectionRequestCard({ entry }: { entry: Entry }) {
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
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12, width: '100%', maxWidth: 480, boxSizing: 'border-box', alignSelf: 'flex-start', padding: '14px 16px', border: '1px solid var(--border)', borderRadius: 20 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 14 }}>
        <RequestLogo request={request} />
        <div style={{ display: 'flex', flexDirection: 'column', gap: 2, flex: 1, minWidth: 0 }}>
          <span style={{ ...font('body', 'semibold'), color: 'var(--text)', ...lineClamp(1) }}>{request.title}</span>
          {entry.data.text ? <span style={{ ...font('subheadline'), color: 'var(--secondary)', ...lineClamp(2) }} title={entry.data.text}>{entry.data.text}</span> : null}
        </div>
        {pending ? (
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, flex: 'none' }}>
            {busy ? <Spinner /> : null}
            <Button disabled={busy || ((isSecret || isLogin) && !value)} onClick={() => void connect()}>
              {isAgent ? 'Sign in' : isLogin ? 'Save' : 'Connect'}
            </Button>
          </div>
        ) : (
          <span style={{ display: 'flex', alignItems: 'center', gap: 6, flex: 'none', ...font('subheadline'), color: 'var(--secondary)' }}>
            {request.status === 'ready' ? <Icon name="checkmark" size={11} weight="semibold" /> : null}
            {request.status === 'ready' ? (isAgent ? 'Signed in' : isLogin ? 'Saved' : 'Connected') : 'Cancelled'}
          </span>
        )}
      </div>
      {pending && isLogin ? (
        <>
          <Field value={username} onChange={setUsername} placeholder="Username or email" />
          <Field value={value} onChange={setValue} placeholder="Password or op:// reference" secret />
          <span style={caption}>Saved securely on this computer. The bot types it into the sign-in page but never sees it.</span>
        </>
      ) : pending && isSecret ? (
        <>
          <span style={caption}>{`${request.field ?? 'Credential'} · ${request.location ?? ''}`}</span>
          <Field value={value} onChange={setValue} placeholder="Credential or op:// reference" secret />
          <span style={caption}>Saved securely on this computer. Never added to the conversation.</span>
        </>
      ) : null}
      {error ? <span style={{ ...font('footnote'), color: 'var(--danger)' }}>{error}</span> : null}
      <FormSheet open={item !== null} onClose={() => setItem(null)}>
        {item ? <InstallConnectorSheet item={item} requestId={entry.id} done={() => setItem(null)} /> : null}
      </FormSheet>
      <FormSheet open={app !== null} onClose={() => setApp(null)}>
        {app ? <ComposioConnectSheet app={app} requestId={entry.id} done={() => setApp(null)} /> : null}
      </FormSheet>
      <FormSheet open={agent !== null} onClose={() => setAgent(null)} width={560} height={560}>
        {agent ? <AgentSheet key={agent.id} initial={agent} recheck /> : null}
      </FormSheet>
    </div>
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
