import { useEffect, useRef, useState } from 'react'
import { useStore } from '../../store/context'
import { errorText } from './drafts'

/** A model's own setting (effort, fast…) and the values the agent accepts. */
export interface ModelOption {
  id: string
  name: string
  values: { id: string; name: string }[]
  default?: string | null
  kind?: string
}

/** Model identifiers and display names advertised by the selected computer's ACP agent. */
export interface AgentModels {
  models: { id: string; name: string; description?: string | null; options?: ModelOption[]; optionsError?: string }[]
  currentModelId?: string | null
  /** The agent offers Fast mode (absent on older hosts). */
  fastMode?: boolean
  defaultModelOptions?: boolean
}

/** What the agent behind `backend` offers, loaded from the selected computer. */
export interface AgentModelsState {
  catalog: AgentModels | null
  loading: boolean
  failure: string | null
  reload: () => void
}

/** Models come from the selected computer's agent, never a bundled provider list. */
export function useAgentModels(backend: string): AgentModelsState {
  const store = useStore()
  const online = store.connection.kind === 'online'
  const [catalog, setCatalog] = useState<AgentModels | null>(null)
  const scope = useRef<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [failure, setFailure] = useState<string | null>(null)
  const [retry, setRetry] = useState(0)
  const custom = backend === 'custom'

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

  return { catalog, loading, failure, reload: () => setRetry((r) => r + 1) }
}
