import { useState } from 'react'
import { MAX_FALLBACKS, moveFallback, restLine, restOf, stepBanner } from '@shared/fallbacks'
import type { Backend, Bot, BotDraft, Fallback } from '@shared/models'
import { AgentIcon } from '../../components/AgentIcon'
import { Button, CardForm, CardSection, DropdownMenu, IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog, ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, IconSlot, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { FormSheet } from '../marketplace/pieces'
import { useAgentModels } from './agent-models'
import { AgentModelPicker } from './AgentModelPicker'
import './bots.css'

const ABOUT =
  "When this bot's agent runs out of usage, is rate limited, signed out or won't start, the next agent here carries on in the same chat, with the same memory and workspace. The bot goes back to its own agent when it recovers. A fallback that uses an API key costs money. To fall back between one agent's models, set that in the agent itself (Claude Code's fallbackModel)."

/** An agent as the Agent pickers list it: its logo, and "(not installed)" when it isn't. */
export const agentChoice = (b: Backend) => ({ id: b.id, label: b.available ? b.name : `${b.name} (not installed)`, leading: <AgentIcon registry={b.registry ?? b.id} size={14} /> })

/** The Agent card's first row while the bot runs on a fallback, with the way back to its own agent. */
export function AgentStepBanner({ bot }: { bot: Bot }) {
  const store = useStore()
  return (
    <HStack gap={8} className="fade-in">
      <AgentIcon registry={store.agentRegistry(bot.agentStep?.agent ?? bot.backend)} size={14} />
      <Text tone="text" grow>{stepBanner(bot, (a) => store.backendName(a), Date.now())}</Text>
      <Button kind="secondary" className="fallback-switch-back" onClick={() => store.switchBack(bot.id)}>Switch back</Button>
    </HStack>
  )
}

/** The Agent card's Fallbacks: the agents that carry on in order, added, moved and removed here, saved with the form. */
export function FallbacksRow({ draft, bot, onChange }: { draft: BotDraft; bot: Bot; onChange: (update: (d: BotDraft) => BotDraft) => void }) {
  const store = useStore()
  const tr = useTr()
  const [adding, setAdding] = useState(false)
  const [about, setAbout] = useState(false)
  const list = draft.fallbacks ?? []
  // Each agent is in the chain once, the bot's own included.
  const choices = (store.hello?.backends ?? []).filter((b) => b.id !== draft.backend && !list.some((f) => f.backend === b.id))
  const edit = (change: (list: Fallback[]) => Fallback[]) => onChange((d) => ({ ...d, fallbacks: change(d.fallbacks ?? []) }))
  const now = Date.now()
  return (
    <VStack gap={8}>
      <HStack gap={2}>
        <Text tone="text">{tr(`Fallbacks`)}</Text>
        <IconButton title="About fallbacks" icon="info.circle" size={22} onClick={() => setAbout(true)} />
        <Spacer />
        {list.length < MAX_FALLBACKS && choices.length ? <IconButton title="Add fallback" icon="plus" className="fade-in" onClick={() => setAdding(true)} /> : null}
      </HStack>
      {list.length ? (
        list.map((f, i) => (
          <FallbackRow
            key={f.backend}
            fallback={f}
            index={i}
            count={list.length}
            bot={bot}
            now={now}
            move={(by) => edit((l) => moveFallback(l, i, by))}
            remove={() => edit((l) => l.filter((_, j) => j !== i))}
          />
        ))
      ) : (
        <Text tone="secondary">{tr(`No fallbacks yet.`)}</Text>
      )}
      <Text size="caption" tone="secondary">
        {tr('If {agent} runs out of usage or can\'t answer, the bot carries on with the next one. Chat, memory and workspace carry over.').replace('{agent}', store.backendName(draft.backend))}
      </Text>
      <Dialog open={about} title={tr('Fallbacks')} message={tr(ABOUT)} actions={[]} cancel="OK" onClose={() => setAbout(false)} />
      <FormSheet open={adding} onClose={() => setAdding(false)} width={460} height={420}>
        {adding ? (
          <AddFallback
            choices={choices}
            onAdd={(f) => {
              edit((l) => [...l, f])
              setAdding(false)
            }}
          />
        ) : null}
      </FormSheet>
    </VStack>
  )
}

/** One fallback: its place, agent and model, whether it's resting or in use, and its menu. */
function FallbackRow({ fallback, index, count, bot, now, move, remove }: {
  fallback: Fallback
  index: number
  count: number
  bot: Bot
  now: number
  move: (by: number) => void
  remove: () => void
}) {
  const store = useStore()
  const tr = useTr()
  const backend = store.hello?.backends.find((b) => b.id === fallback.backend)
  const name = backend ? agentChoice(backend).label : store.backendName(fallback.backend)
  const rest = restOf(bot, fallback.backend, now)
  return (
    <HStack gap={8} className="fade-in">
      <IconSlot width={12} tone="secondary">
        <Text tabular>{index + 1}</Text>
      </IconSlot>
      <AgentIcon registry={store.agentRegistry(fallback.backend)} size={14} />
      <VStack gap={2} grow>
        <Text tone="text" truncate>{`${name} · ${fallback.model || tr(`Default`)}`}</Text>
        {rest ? <Text size="caption" tone="secondary">{restLine(rest, now)}</Text> : null}
      </VStack>
      {bot.agentStep?.agent === fallback.backend ? <Text size="caption" weight="medium" tone="text" className="fallback-chip fade-in">{tr(`In use`)}</Text> : null}
      <DropdownMenu
        className="icon-button fallback-menu"
        title="More"
        items={() => [
          ...(index > 0 ? [{ title: 'Move up', icon: 'arrow.up', action: () => move(-1) }] : []),
          ...(index < count - 1 ? [{ title: 'Move down', icon: 'arrow.down', action: () => move(1) }] : []),
          { title: 'Remove', icon: 'trash', destructive: true, divider: count > 1, action: remove },
        ]}
      >
        <Icon name="ellipsis" size={14} weight="medium" />
      </DropdownMenu>
    </HStack>
  )
}

/** The "Add fallback" sheet: another agent, and the model to use on it. */
function AddFallback({ choices, onAdd }: { choices: Backend[]; onAdd: (f: Fallback) => void }) {
  const tr = useTr()
  const [backend, setBackend] = useState((choices.find((b) => b.available) ?? choices[0])?.id ?? '')
  const [model, setModel] = useState<string | null>(null)
  const [modelOptions, setModelOptions] = useState<Record<string, string>>({})
  const models = useAgentModels(backend)
  // Options belong to one model.
  const pickModel = (m: string | null) => {
    if (m !== model) setModelOptions({})
    setModel(m)
  }
  return (
    <SheetPage>
      <ModalHeader title={tr(`Add fallback`)} trailing={<IconButton title="Add" icon="checkmark" disabled={!backend} onClick={() => onAdd({ backend, model, modelOptions })} />} />
      <CardForm>
        <CardSection>
          <AgentModelPicker backend={backend} models={models} selection={model} onChange={pickModel} options={modelOptions} onOptionsChange={setModelOptions} sessionNote={false} agents={choices.map(agentChoice)} onAgentChange={(id) => { setBackend(id); setModel(null); setModelOptions({}) }} />
        </CardSection>
      </CardForm>
    </SheetPage>
  )
}
