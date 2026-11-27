import { useEffect, useRef, useState } from 'react'
import { useStore } from '../../store/context'
import type { AgentModels } from '@shared/agent-models'
import { errorText } from './drafts'
import { trNow } from '../../lib/l10n.ts'
import { agentModelsRequest, normalizeLoginProfile } from './agent-models-query.ts'

/** What the agent behind `backend` offers, loaded from the selected computer. */
export interface AgentModelsState {
  catalog: AgentModels | null
  loading: boolean
  failure: string | null
  reload: () => void
}

/**
 * A catalog's models as a picker lists them (`options`): the agent's default first, under the id `''`, then the rest,
 * and `selection` itself where the list lacks it. `value` is the one `selection` shows as, `selected` that model, and
 * `modelOptions` its own settings.
 */
export function modelChoices(catalog: AgentModels | null, selection: string | null | undefined, tr: (text: string) => string) {
  const agentDefault = catalog?.models.find((m) => m.name.toLowerCase().startsWith('default'))
  const available = (catalog?.models ?? []).filter((m) => m.id !== agentDefault?.id)
  const current = catalog?.models.find((m) => m.id === catalog.currentModelId)
  const value = selection === agentDefault?.id ? '' : (selection ?? '')
  const selected = catalog?.models.find((m) => m.id === (value || agentDefault?.id || catalog.currentModelId))
  const modelOptions = value || catalog?.defaultModelOptions ? selected?.options ?? [] : []
  const options: { id: string; label: string }[] = [{ id: '', label: agentDefault?.name ?? (current ? tr('Default · {name}').replaceAll('{name}', () => current.name) : tr('Default')) }, ...available.map((m) => ({ id: m.id, label: m.name }))]
  if (value && !available.some((m) => m.id === value)) options.push({ id: value, label: value })
  return { value, selected, modelOptions, options }
}

/**
 * Models come from the selected computer's agent, never a bundled provider list. The list the computer gave last shows
 * at once (`store.modelsFor`, which the host's `models` events keep current); `reload` asks it to read the agent again.
 */
export function useAgentModels(backend: string, loginProfile?: string | null): AgentModelsState {
  const store = useStore()
  const online = store.connection.kind === 'online'
  const [loading, setLoading] = useState(false)
  const [failure, setFailure] = useState<string | null>(null)
  const [retry, setRetry] = useState(0)
  const asked = useRef(0)
  const custom = backend === 'custom'
  const profile = normalizeLoginProfile(loginProfile)
  const catalog = store.modelsFor(backend, profile)

  useEffect(() => {
    setFailure(null)
    setLoading(false)
    if (custom) return
    const client = store.client
    if (!online || !client) {
      setFailure(trNow('Connect to this computer to load its models. You can still enter a model ID.'))
      return
    }
    const refresh = retry !== asked.current
    asked.current = retry
    let cancelled = false
    setLoading(true)
    client
      .call<AgentModels>('agentModels', agentModelsRequest(backend, profile, refresh), 300_000)
      .then((res) => !cancelled && store.keepModels(backend, profile, res))
      .catch((e: unknown) => !cancelled && setFailure(`Couldn't load models: ${errorText(e)}`))
      .finally(() => !cancelled && setLoading(false))
    return () => {
      cancelled = true
    }
  }, [store, store.computer.id, store.client, backend, profile, online, retry, custom])

  return { catalog, loading, failure, reload: () => setRetry((r) => r + 1) }
}
