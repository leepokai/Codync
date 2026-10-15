import { useEffect, useRef, useState } from 'react'
import { ChoicePicker, IconButton, Pill, Spinner } from '../../components/Controls'
import { font } from '../../lib/fonts'
import { useStore } from '../../store/context'
import { errorText } from './drafts'

/** A model's own setting (effort, fast…) and the values the agent accepts. */
interface ModelOption {
  id: string
  name: string
  values: { id: string; name: string }[]
  default?: string | null
}

/** Model identifiers and display names advertised by the selected computer's ACP agent. */
interface AgentModels {
  models: { id: string; name: string; description?: string | null; options?: ModelOption[] }[]
  currentModelId?: string | null
}

/** Models come from the selected computer's agent, never a bundled provider list. */
export function AgentModelPicker({ backend, selection, onChange, options: chosen, onOptionsChange }: {
  backend: string
  selection: string | null | undefined
  onChange: (model: string | null) => void
  options: Record<string, string>
  onOptionsChange: (options: Record<string, string>) => void
}) {
  const store = useStore()
  const online = store.connection.kind === 'online'
  const [catalog, setCatalog] = useState<AgentModels | null>(null)
  const scope = useRef<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [failure, setFailure] = useState<string | null>(null)
  const [retry, setRetry] = useState(0)
  const custom = backend === 'custom'
  const [search, setSearch] = useState('')

  // A different agent starts on its own default model.
  const lastBackend = useRef(backend)
  useEffect(() => {
    if (lastBackend.current === backend) return
    lastBackend.current = backend
    onChange(null)
  }, [backend, onChange])

  useEffect(() => {
    const key = `${store.computer.id}/${backend}`
    if (scope.current !== key) {
      setCatalog(null)
      scope.current = key
    }
    setFailure(null)
    setLoading(false)
    if (custom) return
    const client = store.client
    if (!online || !client) {
      setFailure('Connect to this computer to load its models. You can still enter a model ID.')
      return
    }
    let cancelled = false
    setLoading(true)
    client
      .call<AgentModels>('agentModels', { backend }, 300_000)
      .then((res) => !cancelled && setCatalog(res))
      .catch((e: unknown) => !cancelled && setFailure(`Couldn't load models: ${errorText(e)}`))
      .finally(() => !cancelled && setLoading(false))
    return () => {
      cancelled = true
    }
  }, [store.computer.id, store.client, backend, online, retry, custom])

  // The agent's own "Default (recommended)" entry is the same choice as leaving the model unset,
  // so it folds into the single Default row instead of showing twice.
  const agentDefault = catalog?.models.find((m) => m.name.toLowerCase().startsWith('default'))
  const available = (catalog?.models ?? []).filter((m) => m.id !== agentDefault?.id)
  const current = available.find((m) => m.id === catalog?.currentModelId)?.name
  const matching = available.filter((m) => m.name.toLowerCase().includes(search.toLowerCase()))
  const options = [{ id: '', label: agentDefault?.name ?? (current ? `Default · ${current}` : 'Default') }, ...matching.map((m) => ({ id: m.id, label: m.name }))]
  // The selected model stays listed while the query hides it, under its name from the full list —
  // the pill reads this label, so a raw id would show there.
  if (selection && selection !== agentDefault?.id && !matching.some((m) => m.id === selection)) {
    options.push({ id: selection, label: available.find((m) => m.id === selection)?.name ?? selection })
  }
  const value = selection === agentDefault?.id ? '' : (selection ?? '')
  const set = (v: string) => onChange(v ? v : null)
  const empty = catalog?.models.length === 0
  const modelOptions = available.find((m) => m.id === value)?.options ?? []
  const setOption = (id: string, v: string) => {
    const { [id]: _, ...rest } = chosen
    onOptionsChange(v ? { ...rest, [id]: v } : rest)
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <span style={{ color: 'var(--text)', whiteSpace: 'nowrap' }}>Model</span>
        <span style={{ flex: 1 }} />
        <div style={{ display: 'flex', alignItems: 'center', gap: 4, minWidth: 0 }}>
          {custom ? null : (
            // Loading and refresh share one fixed slot, so the row never grows for loading.
            <div style={{ width: 28, height: 28, display: 'grid', placeItems: 'center', flex: 'none' }}>
              <div style={{ gridArea: '1 / 1', opacity: loading ? 0 : 1, transition: 'opacity var(--fade)', pointerEvents: loading ? 'none' : undefined }}>
                <IconButton title={failure ? 'Retry model list' : 'Refresh model list'} icon="arrow.clockwise" disabled={loading} onClick={() => setRetry((r) => r + 1)} />
              </div>
              {loading ? (
                <div className="fade-in" style={{ gridArea: '1 / 1', display: 'flex' }} aria-label="Loading available models">
                  <Spinner size={12} />
                </div>
              ) : null}
            </div>
          )}
          {custom || (!loading && (!catalog || empty)) ? (
            <Pill outlined style={{ minWidth: 0, maxWidth: 160 }}>
              <input value={value} placeholder="Default" spellCheck={false} onChange={(e) => set(e.target.value)} style={{ textAlign: 'right', width: 142, maxWidth: '100%', minWidth: 0, color: 'var(--text)', userSelect: 'text' }} />
            </Pill>
          ) : (
            <div style={{ display: 'flex', minWidth: 0 }}>
              <ChoicePicker selection={value} options={options} onChange={set} search={{ value: search, onChange: setSearch, placeholder: 'Search models...' }} />
            </div>
          )}
        </div>
      </div>
      {modelOptions.map((o) => {
        const fallback = o.values.find((v) => v.id === o.default)?.name
        const choices = [{ id: '', label: fallback ? `Default · ${fallback}` : 'Default' }, ...o.values.map((v) => ({ id: v.id, label: v.name }))]
        return (
          <div key={o.id} className="fade-in" style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ color: 'var(--text)', whiteSpace: 'nowrap' }}>{o.name}</span>
            <span style={{ flex: 1 }} />
            <ChoicePicker selection={chosen[o.id] ?? ''} options={choices} onChange={(v) => setOption(o.id, v)} />
          </div>
        )
      })}
      {!custom && !loading && failure ? (
        <span style={{ ...font('caption'), color: 'var(--warning)' }}>{failure}</span>
      ) : !custom && !loading && empty ? (
        <span style={{ ...font('caption'), color: 'var(--secondary)' }}>This agent doesn't advertise a model list. Leave Default or enter a model ID.</span>
      ) : null}
      <span style={{ ...font('caption'), color: 'var(--secondary)' }}>Changing the model starts a new agent session. Chat history is kept.</span>
    </div>
  )
}
