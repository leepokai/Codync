import { useEffect, useRef, useState } from 'react'
import type { HostClient } from '../../client/host-client'
import { Button, CardSection, ChoicePicker, Reveal, Spinner, ValueRow } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { Field, SwitchRow } from './parts'

type Backend = '' | 'local' | 'docker' | 'ssh' | 'modal' | 'daytona' | 'vercel_sandbox'
type MemoryProvider = 'native' | 'builtin' | 'honcho'
interface SavedRoute { id: string; label: string; provider: string; model: string }

interface RuntimeConfig {
  enabled: boolean
  root: string
  python: string
  terminal: {
    backend: Backend
    ssh_host: string
    ssh_user: string
    ssh_port: number | null
    ssh_key: string
    docker_image: string
    modal_mode: '' | 'auto' | 'managed' | 'direct'
    vercel_runtime: '' | 'node24' | 'node22' | 'python3.13'
  }
  memoryProvider: MemoryProvider
  delegation: {
    routes?: SavedRoute[]
    provider: string
    model: string
    max_iterations: number | null
    max_concurrent_children: number | null
    child_timeout_seconds: number | null
  }
}

interface Snapshot {
  revision: string | null
  config: RuntimeConfig
  applyAfterCurrentTurn?: boolean
}

interface Status {
  installation: { available: boolean; root?: string; python?: string; error?: string; profileSummary?: string }
}

const BACKENDS: { id: Backend; label: string }[] = [
  { id: '', label: 'Hermes defaults' },
  { id: 'local', label: 'Local' },
  { id: 'docker', label: 'Docker' },
  { id: 'ssh', label: 'SSH' },
  { id: 'modal', label: 'Modal' },
  { id: 'daytona', label: 'Daytona' },
  { id: 'vercel_sandbox', label: 'Vercel Sandbox' },
]

const MEMORY: { id: MemoryProvider; label: string }[] = [
  { id: 'native', label: 'Hermes defaults' },
  { id: 'builtin', label: 'Hermes files' },
  { id: 'honcho', label: 'Honcho' },
]

function message(reason: unknown): string {
  const text = reason instanceof Error ? reason.message : String(reason)
  return /unknown.*(?:method|command)|method not found|not supported/i.test(text)
    ? 'Update the host on this computer to configure the Hermes runtime.'
    : text
}

function Input({ label, detail, value, placeholder, onChange, required = false }: {
  label: string
  detail?: string
  value: string
  placeholder?: string
  onChange: (value: string) => void
  required?: boolean
}) {
  const tr = useTr()
  return (
    <Field label={label} detail={detail}>
      <input className="field-box" aria-label={tr(label)} value={value} placeholder={placeholder ? tr(placeholder) : undefined}
        spellCheck={false} autoComplete="off" maxLength={4096} required={required} onChange={(event) => onChange(event.target.value)} />
    </Field>
  )
}

function NumberInput({ label, value, min, max, onChange }: {
  label: string
  value: number | null
  min: number
  max: number
  onChange: (value: number | null) => void
}) {
  const tr = useTr()
  return (
    <Field label={label}>
      <input className="field-box" aria-label={tr(label)} type="number" inputMode="numeric" min={min} max={max} step={1}
        value={value ?? ''} placeholder={tr('Hermes default')} onChange={(event) => {
          const raw = event.target.value
          onChange(raw === '' ? null : Number(raw))
        }} />
    </Field>
  )
}

/** A separate, explicit Apply: ordinary bot autosaves never overwrite runtime configuration. */
export function HermesRuntimeCard({ botId }: { botId: string }) {
  const tr = useTr()
  const store = useStore()
  const host = store.hello?.computerId ?? store.hello?.hostId
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null)
  const [config, setConfig] = useState<RuntimeConfig | null>(null)
  const [status, setStatus] = useState<Status | null>(null)
  const [statusError, setStatusError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [refreshing, setRefreshing] = useState(false)
  const [advanced, setAdvanced] = useState(false)
  const [reload, setReload] = useState(0)
  const epoch = useRef(0)
  const client = useRef<HostClient | null>(null)
  const online = store.connection.kind === 'online'

  useEffect(() => {
    const current = ++epoch.current
    client.current = null
    setLoading(true)
    setSaving(false)
    setSnapshot(null)
    setConfig(null)
    setStatus(null)
    setError(null)
    setStatusError(null)
    setNotice(null)
    void (async () => {
      try {
        const connection = await store.ready()
        if (epoch.current !== current) return
        const [saved, probe] = await Promise.allSettled([
          connection.call<Snapshot>('hermesRuntime', { botId }),
          connection.call<Status>('hermesRuntimeStatus', { botId }),
        ])
        if (epoch.current !== current) return
        if (saved.status === 'fulfilled') {
          if (!saved.value.config?.terminal || !saved.value.config.delegation) {
            throw new Error('Update the host on this computer to configure the Hermes runtime.')
          }
          setSnapshot(structuredClone(saved.value))
          setConfig(structuredClone(saved.value.config))
          client.current = connection
        } else {
          setError(message(saved.reason))
        }
        if (probe.status === 'fulfilled') setStatus(probe.value)
        else setStatusError(message(probe.reason))
      } catch (reason) {
        if (epoch.current === current) setError(message(reason))
      } finally {
        if (epoch.current === current) setLoading(false)
      }
    })()
    return () => { epoch.current++; client.current = null }
  }, [botId, host, reload, store])

  const refreshStatus = async () => {
    const current = epoch.current
    setRefreshing(true)
    try {
      const connection = client.current ?? await store.ready()
      if (epoch.current !== current) return
      const result = await connection.call<Status>('hermesRuntimeStatus', { botId })
      if (epoch.current !== current) return
      setStatus(result)
      setStatusError(null)
    } catch (reason) {
      if (epoch.current === current) setStatusError(message(reason))
    } finally {
      if (epoch.current === current) setRefreshing(false)
    }
  }

  const apply = async () => {
    if (!config || !snapshot || !client.current || saving || !online || invalidNumbers) return
    const current = epoch.current
    const submitted = structuredClone(config)
    setSaving(true)
    setError(null)
    setNotice(null)
    try {
      const result = await client.current.call<Snapshot>('setHermesRuntime', {
        botId, config: submitted, ifRevision: snapshot.revision,
      })
      if (epoch.current !== current) return
      setSnapshot(structuredClone(result))
      setConfig(structuredClone(result.config))
      setNotice('Saved. Active work keeps its current settings; changes apply on the next turn.')
      void refreshStatus()
    } catch (reason) {
      if (epoch.current === current) setError(message(reason))
    } finally {
      if (epoch.current === current) setSaving(false)
    }
  }

  const checkEnvironment = async () => {
    if (!snapshot?.config.enabled || !client.current || refreshing || saving || !online) return
    const current = epoch.current
    setRefreshing(true)
    try {
      const result = await client.current.call<{ backend: string; status: string; detail: string; revision: string | null }>('hermesEnvironmentHealth', {
        botId, ifRevision: snapshot.revision,
      }, 65_000)
      if (epoch.current !== current) return
      setNotice(`${result.backend}: ${result.status}. ${result.detail}`)
      setError(null)
    } catch (reason) {
      if (epoch.current === current) setError(message(reason))
    } finally {
      if (epoch.current === current) setRefreshing(false)
    }
  }

  const terminal = <K extends keyof RuntimeConfig['terminal']>(key: K, value: RuntimeConfig['terminal'][K]) =>
    setConfig((current) => current ? { ...current, terminal: { ...current.terminal, [key]: value } } : current)
  const delegation = <K extends keyof RuntimeConfig['delegation']>(key: K, value: RuntimeConfig['delegation'][K]) =>
    setConfig((current) => current ? { ...current, delegation: { ...current.delegation, [key]: value } } : current)
  const dirty = config !== null && snapshot !== null && JSON.stringify(config) !== JSON.stringify(snapshot.config)
  const invalidNumbers = config !== null && [
    config.terminal.ssh_port,
    config.delegation.max_iterations,
    config.delegation.max_concurrent_children,
    config.delegation.child_timeout_seconds,
  ].some((value) => value !== null && !Number.isSafeInteger(value))
  const installation = status?.installation
  useEffect(() => { if (dirty) setNotice(null) }, [dirty])

  return (
    <CardSection title="Hermes runtime" footer="Uses the existing Hermes profile and its connections. Installation availability does not verify remote environments.">
      <ValueRow label="Installation">
        {loading || refreshing ? <Spinner /> : <Text size="caption" tone={installation?.available ? 'secondary' : 'tertiary'}>
          {tr(installation ? (installation.available ? 'Available' : 'Unavailable') : 'Not checked')}
        </Text>}
        <Button kind="secondary" disabled={loading || refreshing || saving || !online} onClick={() => void refreshStatus()}>Refresh status</Button>
      </ValueRow>
      {installation?.error || statusError ? <Text size="caption" role="status" tone="warning">{installation?.error ?? statusError}</Text> : null}
      <Button kind="secondary" disabled={!snapshot?.config.enabled || loading || refreshing || saving || !online} onClick={() => void checkEnvironment()}>Check saved environment (may start sandbox)</Button>
      {config ? <fieldset disabled={saving || !online} aria-label={tr('Hermes runtime settings')} style={{ border: 0, padding: 0, margin: 0, minWidth: 0 }}>
        <VStack gap={16}>
          <SwitchRow title="Use runtime settings" detail="Keep active work on its current settings. Apply changes together for the next turn."
            on={config.enabled} onChange={(enabled) => setConfig((current) => current ? { ...current, enabled } : current)} />
          <ValueRow label="Environment">
            <ChoicePicker selection={config.terminal.backend} options={BACKENDS} onChange={(value) => terminal('backend', value)} />
          </ValueRow>
          <Reveal show={config.terminal.backend === 'ssh'}><VStack gap={12}>
            <Input label="SSH host" value={config.terminal.ssh_host} placeholder="Server or SSH configuration alias" required onChange={(value) => terminal('ssh_host', value)} />
            <Input label="SSH user" value={config.terminal.ssh_user} required onChange={(value) => terminal('ssh_user', value)} />
            <NumberInput label="SSH port" value={config.terminal.ssh_port} min={1} max={65535} onChange={(value) => terminal('ssh_port', value)} />
            <Input label="SSH key path" value={config.terminal.ssh_key} placeholder="Use existing SSH authentication" onChange={(value) => terminal('ssh_key', value)} />
          </VStack></Reveal>
          <Reveal show={config.terminal.backend === 'docker'}>
            <Input label="Docker image" value={config.terminal.docker_image} placeholder="Hermes default" onChange={(value) => terminal('docker_image', value)} />
          </Reveal>
          <Reveal show={config.terminal.backend === 'modal'}><ValueRow label="Modal mode">
            <ChoicePicker<RuntimeConfig['terminal']['modal_mode']> selection={config.terminal.modal_mode} options={[
              { id: '', label: 'Hermes default' }, { id: 'auto', label: 'Automatic' }, { id: 'managed', label: 'Managed' }, { id: 'direct', label: 'Direct' },
            ]} onChange={(value) => terminal('modal_mode', value)} />
          </ValueRow></Reveal>
          <Reveal show={config.terminal.backend === 'vercel_sandbox'}><ValueRow label="Vercel runtime">
            <ChoicePicker<RuntimeConfig['terminal']['vercel_runtime']> selection={config.terminal.vercel_runtime} options={[
              { id: '', label: 'Hermes default' }, { id: 'node24', label: 'Node 24' }, { id: 'node22', label: 'Node 22' }, { id: 'python3.13', label: 'Python 3.13' },
            ]} onChange={(value) => terminal('vercel_runtime', value)} />
          </ValueRow></Reveal>
          <ValueRow label="Memory" detail="Honcho uses the existing connection in this Hermes profile.">
            <ChoicePicker selection={config.memoryProvider} options={MEMORY} onChange={(memoryProvider) => setConfig((current) => current ? { ...current, memoryProvider } : current)} />
          </ValueRow>
          <VStack gap={12}>
            <Text size="body" weight="semibold">{tr('Delegation')}</Text>
            <Input label="Provider" value={config.delegation.provider} placeholder="Hermes default" onChange={(value) => delegation('provider', value)} />
            <Input label="Model" value={config.delegation.model} placeholder="Hermes default" onChange={(value) => delegation('model', value)} />
            <NumberInput label="Maximum iterations" value={config.delegation.max_iterations} min={1} max={200} onChange={(value) => delegation('max_iterations', value)} />
            <NumberInput label="Concurrent children" value={config.delegation.max_concurrent_children} min={1} max={10} onChange={(value) => delegation('max_concurrent_children', value)} />
            <NumberInput label="Child inactivity timeout (seconds)" value={config.delegation.child_timeout_seconds} min={30} max={3600} onChange={(value) => delegation('child_timeout_seconds', value)} />
            <Text size="caption" tone="secondary">{tr('Saved task routes let the agent choose a provider and model per task. Each child keeps the iteration and concurrency limits above.')}</Text>
            {(config.delegation.routes ?? []).map((route) => <VStack key={route.id} gap={8}>
              {(['label', 'provider', 'model'] as const).map((key) => <Input key={key} label={key === 'label' ? 'Route name' : key === 'provider' ? 'Route provider' : 'Route model'} value={route[key]}
                onChange={(value) => delegation('routes', (config.delegation.routes ?? []).map((saved) => saved.id === route.id ? { ...saved, [key]: value } : saved))} />)}
              <Button kind="secondary" onClick={() => delegation('routes', (config.delegation.routes ?? []).filter((saved) => saved.id !== route.id))}>Remove route</Button>
            </VStack>)}
            {(config.delegation.routes ?? []).length < 16 ? <Button kind="secondary" onClick={() => delegation('routes', [...(config.delegation.routes ?? []), { id: crypto.randomUUID(), label: '', provider: '', model: '' }])}>Add task route</Button> : null}
          </VStack>
          <VStack gap={12}>
            <Button kind="secondary" aria-expanded={advanced} onClick={() => setAdvanced((value) => !value)}>Advanced installation</Button>
            <Reveal show={advanced}><VStack gap={12}>
              <Input label="Hermes source folder" value={config.root} placeholder={installation?.root ?? 'Auto-detect'} onChange={(root) => setConfig((current) => current ? { ...current, root } : current)} />
              <Input label="Hermes Python path" value={config.python} placeholder={installation?.python ?? 'Auto-detect'} onChange={(python) => setConfig((current) => current ? { ...current, python } : current)} />
            </VStack></Reveal>
          </VStack>
        </VStack>
      </fieldset> : loading ? <Text size="caption" tone="secondary">{tr('Loading runtime settings…')}</Text> : null}
      {error ? <Text size="caption" role="alert" tone="danger">{tr(error)}</Text> : null}
      {invalidNumbers ? <Text size="caption" role="alert" tone="danger">{tr('Enter whole numbers, or clear a numeric field to use the Hermes default.')}</Text> : null}
      {installation?.profileSummary ? <Text size="caption" role="status" tone="secondary">{installation.profileSummary}</Text> : null}
      {notice ? <Text size="caption" role="status" tone="secondary">{tr(notice)}</Text> : null}
      {!online ? <Text size="caption" tone="secondary">{tr('Connect to this computer to edit runtime settings.')}</Text> : null}
      <HStack gap={8}>
        <Button disabled={!online || !dirty || saving || loading || invalidNumbers} onClick={() => void apply()}>{saving ? <Spinner /> : tr('Apply')}</Button>
        <Button kind="secondary" disabled={!online || saving || loading} onClick={() => setReload((value) => value + 1)}>Reload saved settings</Button>
      </HStack>
    </CardSection>
  )
}
