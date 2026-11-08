import { useEffect, useState } from 'react'
import { Button, IconButton } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'

interface EnvNames {
  own: string[]
  inherited: string[]
}

/**
 * Keys this bot keeps for itself, above the ones its agent has for every bot. The agent only uses the names;
 * values stay in the computer's vault.
 */
export function BotAgentKeys({ backend, botId }: { backend: string; botId?: string }) {
  const tr = useTr()
  const store = useStore()
  const [names, setNames] = useState<EnvNames>({ own: [], inherited: [] })
  const [key, setKey] = useState('')
  const [value, setValue] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const load = async () => {
    if (!botId) return
    try {
      setNames(await store.agentEnv(backend, botId))
      setError(null)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }
  useEffect(() => { void load() }, [backend, botId])
  if (!botId) return null
  const save = async (vars: Record<string, string>) => {
    setBusy(true)
    try {
      await store.setAgentEnv(backend, vars, botId)
      setKey('')
      setValue('')
      await load()
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
    setBusy(false)
  }
  return (
    <VStack gap={6} className="settings-stack">
      <Text size="caption" tone="secondary">{tr('Keys for this bot. They override the agent keys below and stay in this computer.')}</Text>
      {names.own.map((name) => (
        <HStack key={name} gap={8}>
          <Text size="caption" grow>{name}</Text>
          <IconButton title={tr('Remove {name}').replace('{name}', name)} icon="trash" disabled={busy} onClick={() => void save({ [name]: '' })} />
        </HStack>
      ))}
      <HStack gap={6}>
        <input className="field-box" value={key} placeholder="VARIABLE" aria-label={tr('Variable name')} spellCheck={false}
          onChange={(e) => setKey(e.target.value.toUpperCase().replace(/[^A-Z0-9_]/g, '_'))} />
        <input className="field-box" value={value} placeholder={tr('Value')} aria-label={tr('Value')} type="password" spellCheck={false}
          onChange={(e) => setValue(e.target.value)} />
        <Button kind="secondary" disabled={busy || !key || !value} onClick={() => void save({ [key]: value })}>{tr('Save key')}</Button>
      </HStack>
      {names.inherited.length ? <Text size="caption" tone="tertiary">{tr('Agent keys, for every bot')}: {names.inherited.join(', ')}</Text> : null}
      {error ? <Text size="caption" role="alert" tone="danger">{error}</Text> : null}
    </VStack>
  )
}
