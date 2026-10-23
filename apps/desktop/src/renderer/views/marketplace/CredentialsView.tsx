import { useEffect, useState } from 'react'
import { Button, CardForm, CardSection, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage, useDismiss } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { errorText, market, type CredentialStatus, type InstalledConnector, type SavedLogin } from './market-models'
import { ErrorText, Field, FormSheet, ScreenHeader, filled } from './pieces'
import { usePlugins } from './plugins'
import { useTr } from '../../lib/use-language'
import './credentials.css'

export function CredentialsView({ onBack }: { onBack: () => void }) {
  const dismiss = useDismiss()
  const store = useStore()
  const tr = useTr()
  const plugins = usePlugins(store)
  const [selected, setSelected] = useState<InstalledConnector | null>(null)
  const [token, setToken] = useState('')
  const [status, setStatus] = useState<CredentialStatus | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    void (async () => {
      await plugins.refresh()
      const client = store.client
      if (!client) return
      try {
        setStatus(await market.credentialStatus(client))
      } catch (e) {
        setError(errorText(e))
      }
    })()
  }, [plugins, store])

  const save = async (value: string) => {
    const client = store.client
    if (!client) return
    setBusy(true)
    setError(null)
    try {
      setStatus(await market.setOnePasswordToken(client, value))
      setToken('')
    } catch (e) {
      setError(errorText(e))
    }
    setBusy(false)
  }

  return (
    <SheetPage className="credentials-page">
      <ScreenHeader title="Credentials" onBack={onBack} trailing={<IconButton title="Close" icon="xmark" onClick={dismiss} />} />
      <CardForm>
        <CardSection title="Storage">
          <div className="credential-storage"><Icon name="lock.shield" size={20} /><div><strong>{tr(status?.provider ?? 'System credential store')}</strong><p>{tr('Encrypted on the connected computer.')}</p></div></div>
        </CardSection>
        <CardSection title="Saved connections">
          {plugins.installedConnectors.filter((c) => c.keys.length > 0).map((c) => (
            <button key={c.id} className="plain-row-button" onClick={() => setSelected(c)}>
              <span className="credential-row-title">{c.name}</span>
              <Icon name="chevron.right" size={10} />
            </button>
          ))}
          {!plugins.installedConnectors.some((c) => c.keys.length > 0) ? <Text size="caption" tone="secondary">{tr(`No saved connection credentials yet.`)}</Text> : null}
          <Text size="caption" tone="secondary">{tr(`Values stay hidden. Select a connection to replace its credentials.`)}</Text>
        </CardSection>
        <LoginsSection />
        <CardSection title="1Password" footer="Use a service account with read access only to a dedicated shared vault. The 1Password CLI must be installed on the computer.">
          <span>{tr(status?.onePasswordConnected ? 'Connected' : 'Connect a shared vault')}</span>
          <CredentialField label="Service account token" value={token} onChange={setToken} placeholder="Paste your token" secret />
          <div><Button disabled={busy || !token} onClick={() => void save(token)}>Connect</Button></div>
          {status?.onePasswordConnected ? <div><Button kind="secondary" disabled={busy} onClick={() => void save('')}>Disconnect</Button></div> : null}
          <Text size="footnote" tone="secondary">{tr(`Use op://vault/item/field in a connector's credential field. Codync retrieves the value when the connector runs.`)}</Text>
        </CardSection>
        {busy ? <Spinner /> : null}
        {error ? <ErrorText text={error} /> : null}
      </CardForm>
      <FormSheet open={selected !== null} onClose={() => setSelected(null)}>
        {selected ? <ConnectorCredentialEditor key={selected.id} connector={selected} /> : null}
      </FormSheet>
    </SheetPage>
  )
}

/** Website and app logins bots type through the computer tool; passwords stay hidden. */
function LoginsSection() {
  const store = useStore()
  const [logins, setLogins] = useState<SavedLogin[]>([])
  const [site, setSite] = useState('')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = async () => {
    const client = store.client
    try {
      setLogins(client ? await market.logins(client) : [])
    } catch (e) {
      setError(errorText(e))
    }
  }

  useEffect(() => {
    void load()
  }, [])

  const save = async () => {
    const client = store.client
    if (!client) return
    setBusy(true)
    setError(null)
    try {
      await market.saveLogin(client, site, username, password)
      setSite('')
      setUsername('')
      setPassword('')
      await load()
    } catch (e) {
      setError(errorText(e))
    }
    setBusy(false)
  }

  const remove = async (login: SavedLogin) => {
    const client = store.client
    if (!client) return
    try {
      await market.removeLogin(client, login.id)
      setLogins((old) => old.filter((l) => l.id !== login.id))
    } catch (e) {
      setError(errorText(e))
    }
  }

  return (
    <CardSection title="Sign-ins" footer="Bots type these into the matching website or app and never see the password. You can use an op:// reference.">
      {logins.map((login) => (
        <HStack key={login.id}>
          <VStack gap={2} grow>
            <span>{login.site}</span>
            {login.username ? <Text size="caption" tone="secondary">{login.username}</Text> : null}
          </VStack>
          <IconButton title="Remove" icon="trash" onClick={() => void remove(login)} />
        </HStack>
      ))}
      <CredentialField label="Website or app" value={site} onChange={setSite} placeholder="github.com" />
      <CredentialField label="Username or email" value={username} onChange={setUsername} placeholder="name@example.com" />
      <CredentialField label="Password" value={password} onChange={setPassword} placeholder="Password or op:// reference" secret />
      <div><Button disabled={busy || !site || !password} onClick={() => void save()}>Save sign-in</Button></div>
      {error ? <Text size="footnote" tone="danger">{error}</Text> : null}
    </CardSection>
  )
}

function ConnectorCredentialEditor({ connector }: { connector: InstalledConnector }) {
  const store = useStore()
  const dismiss = useDismiss()
  const [fields, setFields] = useState<Record<string, string>>({})
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const save = async () => {
    const client = store.client
    if (!client) return
    setSaving(true)
    setError(null)
    try {
      await market.updateConnectorCredentials(client, connector.id, filled(fields))
      setFields({})
      dismiss()
    } catch (e) {
      setError(errorText(e))
    }
    setSaving(false)
  }

  return (
    <SheetPage>
      <ModalHeader
        icon="key"
        title={connector.name}
        trailing={saving ? <Spinner /> : <IconButton title="Save credentials" icon="checkmark" disabled={Object.values(fields).every((v) => !v)} onClick={() => void save()} />}
      />
      <CardForm>
        <CardSection footer="Enter only the values you want to replace. You can use an op:// reference.">
          {connector.keys.map((key) => (
            <Field key={key} label={key} value={fields[key] ?? ''} placeholder={key} secret onChange={(v) => setFields((old) => ({ ...old, [key]: v }))} />
          ))}
        </CardSection>
        {error ? <ErrorText text={error} /> : null}
      </CardForm>
    </SheetPage>
  )
}

/** Persistent labels keep a filled credential field identifiable. */
function CredentialField({ label, ...props }: Parameters<typeof Field>[0] & { label: string }) {
  const tr = useTr()
  return <label className="credential-field"><span>{tr(label)}</span><Field {...props} label={label} /></label>
}
