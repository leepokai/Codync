import { useEffect, useState } from 'react'
import { Button, CardSection, ValueRow } from '../../components/Controls'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useApp } from '../../store/context'
import { Field } from '../marketplace/pieces'

interface Enterprise {
  configured: boolean
  issuer: string | null
  clientId: string | null
  domains: string[] | null
  scimTokenSet: boolean
}

/**
 * A company's OpenID Connect sign-in and the SCIM token for user sync. The provider is checked by its discovery
 * document before it is saved; the SCIM endpoint lists the owner and this computer's paired devices.
 */
export function EnterpriseSection() {
  const tr = useTr()
  const app = useApp()
  const [status, setStatus] = useState<Enterprise | null>(null)
  const [issuer, setIssuer] = useState('')
  const [clientId, setClientId] = useState('')
  const [domains, setDomains] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [token, setToken] = useState<string | null>(null)
  const client = async () => {
    const local = app.local
    return local ? (await local.ready()) : null
  }
  const load = async () => {
    const c = await client()
    if (!c) return
    try {
      setStatus(await c.call<Enterprise>('enterpriseStatus', {}))
      setError(null)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }
  useEffect(() => { void load() }, [app])
  const save = async () => {
    const c = await client()
    if (!c) return
    setBusy(true)
    setError(null)
    try {
      setStatus(await c.call<Enterprise>('setEnterprise', {
        issuer: issuer.trim(),
        clientId: clientId.trim(),
        domains: domains.split(/[\s,]+/).filter(Boolean),
      }))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
    setBusy(false)
  }
  if (!status) return null
  if (status.configured) {
    return (
      <CardSection title={tr('Company sign-in')} footer={tr('People with an email at these domains sign in through the company. SCIM lists the owner and this computer\'s devices; it is read-only until member accounts exist.')}>
        <ValueRow label={tr('Issuer')}>{status.issuer}</ValueRow>
        <ValueRow label={tr('Client id')}>{status.clientId}</ValueRow>
        <ValueRow label={tr('Email domains')}>{(status.domains ?? []).join(', ')}</ValueRow>
        <HStack gap={8} wrap>
          <Button kind="secondary" disabled={busy} onClick={async () => {
            const c = await client()
            if (!c) return
            setBusy(true)
            try { setToken((await c.call<{ token: string }>('rotateScim', {})).token) } finally { setBusy(false) }
          }}>{tr('New SCIM token')}</Button>
          <Button kind="secondary" disabled={busy} onClick={() => window.codync.app.copy(`http://127.0.0.1:${window.codync.hostPort}/scim/v2/Users`)}>{tr('Copy SCIM URL')}</Button>
          <Button kind="secondary" disabled={busy} onClick={async () => {
            const c = await client()
            if (c) setStatus(await c.call<Enterprise>('disconnectEnterprise', {}))
          }}>{tr('Disconnect')}</Button>
        </HStack>
        {token ? <Text size="caption" tone="attention">{tr('SCIM token (shown once)')}: {token}</Text> : null}
        {error ? <Text role="alert" tone="danger">{error}</Text> : null}
      </CardSection>
    )
  }
  return (
    <CardSection title={tr('Company sign-in')} footer={tr('An OpenID Connect provider such as Microsoft Entra ID, Okta or Google Workspace. Its discovery document is checked before saving.')}>
      <Field label={tr('Issuer')} value={issuer} onChange={setIssuer} placeholder="https://login.microsoftonline.com/<tenant>/v2.0" />
      <Field label={tr('Client id')} value={clientId} onChange={setClientId} />
      <Field label={tr('Email domains')} value={domains} onChange={setDomains} placeholder="example.com, example.co.uk" />
      <Button disabled={busy || !issuer.trim() || !clientId.trim()} onClick={() => void save()}>{busy ? tr('Checking…') : tr('Connect provider')}</Button>
      {error ? <Text role="alert" tone="danger">{error}</Text> : null}
    </CardSection>
  )
}
