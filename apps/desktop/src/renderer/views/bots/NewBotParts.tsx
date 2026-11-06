import { useCallback } from 'react'
import type { Backend, BotDraft } from '@shared/models'
import type { BotProfile } from '@shared/catalog'
import { CharacterAvatar } from '../../components/Avatar'
import { ChoicePicker, ValueRow } from '../../components/Controls'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { useTr } from '../../lib/use-language'
import { AgentModelPicker } from './AgentModelPicker'
import { AgentAccountPicker } from './AgentAccountPicker'
import { useAgentModels } from './agent-models'
import { agentChoice } from './Fallbacks'
import { AutoTextArea, Field } from './parts'

type Change = (update: (d: BotDraft) => BotDraft) => void

/** What the bot is for: its standing instructions, in plain words. */
export function PurposeField({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return (
    <Field label="What should this bot help with?">
      <AutoTextArea className="field-box" value={value} placeholder="Plan trips, compare options or help with daily work." minRows={2} maxRows={6} onChange={onChange} />
    </Field>
  )
}

/** Why the picked agent can't answer yet, if it can't. */
export function agentNotice(backend: Backend | undefined): string | null {
  if (!backend) return null
  if (!backend.available) return 'Not installed on this computer. Set it up in Extensions.'
  if (backend.signedIn === false) return 'Not signed in. The bot asks you to sign in before its first message.'
  return null
}

/** The agent and its model, with the agent's state when it isn't ready. */
export function AgentAndModel({ draft, onChange }: { draft: BotDraft; onChange: Change }) {
  const store = useStore()
  const tr = useTr()
  const backends = store.hello?.backends ?? []
  const backend = backends.find((b) => b.id === draft.backend)
  const models = useAgentModels(draft.backend, draft.account ?? null)
  const setModel = useCallback((model: string | null) => onChange((d) => (d.model === model ? d : { ...d, model, modelOptions: {} })), [onChange])
  const setOptions = useCallback((modelOptions: Record<string, string>) => onChange((d) => ({ ...d, modelOptions })), [onChange])
  const notice = agentNotice(backend)
  return (
    <VStack gap={10} className="create-fields">
      <ValueRow label="Agent">
        <ChoicePicker
          selection={draft.backend}
          options={backends.filter((b) => b.available || b.curated === true).map(agentChoice)}
          onChange={(id) => onChange((d) => ({ ...d, backend: id, account: null, model: null, modelOptions: {} }))}
        />
      </ValueRow>
      <AgentAccountPicker backend={draft.backend} name={backend?.name ?? draft.backend} selection={draft.account}
        online={store.connection.kind === 'online'} contextKey={JSON.stringify([store.contextId, store.computer.id, draft.id ?? 'new'])}
        load={() => store.agentAccounts(draft.backend)} supported={backend?.accounts !== false}
        authenticate={() => store.authenticateAgentAccount(draft.backend)} remove={(id) => store.deleteAgentAccount(id)}
        signOut={backend?.signOut ? () => store.signOutAgent(draft.backend) : undefined}
        onChange={(id) => onChange((d) => (d.account === id ? d : { ...d, account: id, model: null, modelOptions: {} }))} />
      {notice ? <Text size="caption" tone="attention" className="fade-in">{tr(notice)}</Text> : null}
      <AgentModelPicker backend={draft.backend} models={models} selection={draft.model} onChange={setModel} options={draft.modelOptions ?? {}} onOptionsChange={setOptions} sessionNote={false} />
    </VStack>
  )
}

/** Ready-made starting points: picking one fills the name, the look and the instructions. */
export function Suggestions({ profiles, onPick }: { profiles: BotProfile[]; onPick: (profile: BotProfile) => void }) {
  const tr = useTr()
  if (profiles.length === 0) return null
  return (
    <VStack gap={8} className="create-fields fade-in">
      <Text size="footnote" weight="semibold" tone="secondary">{tr(`Suggestions`)}</Text>
      {profiles.map((p) => (
        <button key={p.id} className="press create-suggestion" onClick={() => onPick(p)}>
          <CharacterAvatar shape={p.avatarShape} color={p.avatarColor} size={32} />
          <VStack as="span" align="flex-start" gap={2} shrink>
            <Text weight="semibold" tone="text" truncate>{p.name}</Text>
            <Text size="caption" tone="secondary" lines={2}>{p.summary}</Text>
          </VStack>
        </button>
      ))}
    </VStack>
  )
}
