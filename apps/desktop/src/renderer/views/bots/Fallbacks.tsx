import { useState } from 'react'
import { freeLogins, MAX_FALLBACKS, moveFallback, restLine, restOf, runningAgent, sameLogin, stepBannerPhrase, stepLogin, type Login } from '@shared/fallbacks'
import type { Backend, Bot, BotDraft, Fallback } from '@shared/models'
import { AgentIcon } from '../../components/AgentIcon'
import { Button, CardForm, CardSection, DropdownMenu, IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog, ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, IconSlot, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { fill } from '../../lib/fill'
import { useStore } from '../../store/context'
import { FormSheet } from '../marketplace/pieces'
import { useAgentModels } from './agent-models'
import { LoginProfileRow } from './LoginProfileRow'
import { AgentModelPicker } from './AgentModelPicker'
import './bots.css'

const ABOUT =
  "When this bot's agent runs out of usage, is rate limited, signed out or won't start, the next agent here carries on in the same chat, with the same memory and workspace. The bot goes back to its own agent when it recovers. A fallback that uses an API key costs money. To fall back between one agent's models, set that in the agent itself (Claude Code's fallbackModel)."

/** An agent as the Agent pickers list it: its logo, and "(not installed)" when it isn't. */
export const agentChoice = (b: Backend) => ({ id: b.id, label: b.available ? b.name : `${b.name} (not installed)`, leading: <AgentIcon registry={b.registry ?? b.id} size={14} /> })

/** The Agent card's first row while the bot runs on a fallback, with the way back to its own agent. */
export function AgentStepBanner({ bot }: { bot: Bot }) {
  const store = useStore()
  const tr = useTr()
  // "Claude Code (Work)": a login on a login profile, and the computer's own when it's asked for by name.
  const name = (login: Login, own: boolean) => {
    const backend = store.hello?.backends.find((b) => b.id === login.backend)
    const profile = login.loginProfile
      ? backend?.loginProfiles?.find((p) => p.id === login.loginProfile)?.name ?? tr('New login profile')
      : own ? backend?.defaultLoginProfile?.name ?? tr('Default login profile') : null
    return profile ? `${store.backendName(login.backend)} (${profile})` : store.backendName(login.backend)
  }
  // The phrase is translated before its names go in, so a login profile named "Work while traveling" stays one name.
  const banner = stepBannerPhrase(bot, name, Date.now())
  return (
    <HStack gap={8} className="fade-in">
      <AgentIcon registry={store.agentRegistry(runningAgent(bot))} size={14} />
      <Text tone="text" grow>{banner === null ? null : fill(tr(banner.key), banner.values)}</Text>
      <Button kind="secondary" className="fallback-switch-back" onClick={() => store.switchBack(bot.id)}>Switch back</Button>
    </HStack>
  )
}

/** The Agent card's Fallbacks: the agents that carry on in order, added, moved, edited and removed here, saved with the form. */
export function FallbacksRow({ draft, bot, onChange }: { draft: BotDraft; bot: Bot; onChange: (update: (d: BotDraft) => BotDraft) => void }) {
  const store = useStore()
  const tr = useTr()
  // The step the sheet adds (list.length) or edits in place.
  const [sheet, setSheet] = useState<number | null>(null)
  const [about, setAbout] = useState(false)
  const list = draft.fallbacks ?? []
  // Each login is in the chain once, the bot's own included: an agent comes again only on another of its login profiles.
  // The step being edited keeps its own login, and its agent stays a choice even when that login has signed out since.
  const takenBut = (index: number): Login[] => [draft, ...list.filter((_, j) => j !== index)]
  const choicesBut = (index: number) => (store.hello?.backends ?? []).filter((b) => b.id === list[index]?.backend || freeLogins(b, takenBut(index)).length)
  const choices = choicesBut(list.length)
  const update = (change: (list: Fallback[]) => Fallback[]) => onChange((d) => ({ ...d, fallbacks: change(d.fallbacks ?? []) }))
  const now = Date.now()
  return (
    <VStack gap={8}>
      <HStack gap={2}>
        <Text tone="text">{tr(`Fallbacks`)}</Text>
        <IconButton title="About fallbacks" icon="info.circle" size={22} onClick={() => setAbout(true)} />
        <Spacer />
        {list.length < MAX_FALLBACKS && choices.length ? <IconButton title="Add fallback" icon="plus" className="fade-in" onClick={() => setSheet(list.length)} /> : null}
      </HStack>
      {list.length ? (
        list.map((f, i) => (
          <FallbackRow
            key={`${f.backend}#${f.loginProfile ?? ''}`}
            fallback={f}
            index={i}
            count={list.length}
            bot={bot}
            now={now}
            move={(by) => update((l) => moveFallback(l, i, by))}
            onEdit={() => setSheet(i)}
            remove={() => update((l) => l.filter((_, j) => j !== i))}
          />
        ))
      ) : (
        <Text tone="secondary">{tr(`No fallbacks yet.`)}</Text>
      )}
      <Text size="caption" tone="secondary">
        {tr('If {agent} runs out of usage or can\'t answer, the bot carries on with the next one. Chat, memory and workspace carry over.').replace('{agent}', store.backendName(draft.backend))}
      </Text>
      <Dialog open={about} title={tr('Fallbacks')} message={tr(ABOUT)} actions={[]} cancel="OK" onClose={() => setAbout(false)} />
      <FormSheet open={sheet !== null} onClose={() => setSheet(null)} width={460} height={420}>
        {sheet !== null ? (
          <FallbackSheet
            choices={choicesBut(sheet)}
            taken={takenBut(sheet)}
            initial={list[sheet]}
            onSave={(f) => {
              update((l) => l.toSpliced(sheet, 1, f))
              setSheet(null)
            }}
          />
        ) : null}
      </FormSheet>
    </VStack>
  )
}

/** One fallback: its place, agent and model, whether it's resting or in use, and its menu. */
function FallbackRow({ fallback, index, count, bot, now, move, onEdit, remove }: {
  fallback: Fallback
  index: number
  count: number
  bot: Bot
  now: number
  move: (by: number) => void
  onEdit: () => void
  remove: () => void
}) {
  const store = useStore()
  const tr = useTr()
  const backend = store.hello?.backends.find((b) => b.id === fallback.backend)
  const name = backend ? agentChoice(backend).label : store.backendName(fallback.backend)
  const rest = restOf(bot, fallback, now)
  const profile = fallback.loginProfile ? backend?.loginProfiles?.find((p) => p.id === fallback.loginProfile) : null
  const stepName = profile ? `${name} (${profile.name ?? tr('New login profile')})` : name
  const step = stepLogin(bot)
  return (
    <HStack gap={8} className="fade-in">
      <IconSlot width={12} tone="secondary">
        <Text tabular>{index + 1}</Text>
      </IconSlot>
      <AgentIcon registry={store.agentRegistry(fallback.backend)} size={14} />
      <VStack gap={2} grow>
        <Text tone="text" truncate>{`${stepName} · ${fallback.model || tr(`Default`)}`}</Text>
        {rest ? <Text size="caption" tone="secondary">{tr(restLine(rest, now))}</Text> : null}
      </VStack>
      {step && sameLogin(step, fallback) ? <Text size="caption" weight="medium" tone="text" className="fallback-chip fade-in">{tr(`In use`)}</Text> : null}
      <DropdownMenu
        className="icon-button fallback-menu"
        title={tr('More')}
        items={() => [
          ...(index > 0 ? [{ title: 'Move up', icon: 'arrow.up', action: () => move(-1) }] : []),
          ...(index < count - 1 ? [{ title: 'Move down', icon: 'arrow.down', action: () => move(1) }] : []),
          { title: 'Edit', icon: 'pencil', action: onEdit },
          { title: 'Remove', icon: 'trash', destructive: true, divider: true, action: remove },
        ]}
      >
        <Icon name="ellipsis" size={14} weight="medium" />
      </DropdownMenu>
    </HStack>
  )
}

/** The "Add fallback" sheet, or "Edit fallback" with a step's own (`initial`): another agent (or this one on another of its login profiles), and the model to use on it. */
function FallbackSheet({ choices, taken, initial, onSave }: { choices: Backend[]; taken: Login[]; initial?: Fallback; onSave: (f: Fallback) => void }) {
  const tr = useTr()
  const [backend, setBackend] = useState(initial?.backend ?? (choices.find((b) => b.available) ?? choices[0])?.id ?? '')
  const agent = choices.find((b) => b.id === backend)
  // The step's own while its agent stays the same.
  const ownOf = (id: string) => (id === initial?.backend ? initial : undefined)
  const own = ownOf(backend)
  const open = agent ? freeLogins(agent, taken) : []
  // A step keeps its own login, even one that has signed out since.
  const free = own && !open.includes(own.loginProfile ?? null) ? [...open, own.loginProfile ?? null] : open
  // The first login the chain doesn't have yet, until the user picks one.
  const [picked, setPicked] = useState<string | null | undefined>(initial ? initial.loginProfile ?? null : undefined)
  const loginProfile = picked !== undefined && free.includes(picked) ? picked : free[0] ?? null
  const [model, setModel] = useState<string | null>(initial?.model ?? null)
  const [modelOptions, setModelOptions] = useState<Record<string, string>>(initial?.modelOptions ?? {})
  const models = useAgentModels(backend)
  // Options belong to one model.
  const pickModel = (m: string | null) => {
    if (m !== model) setModelOptions({})
    setModel(m)
  }
  // Back on the step's own agent, its model and login come back with it.
  const pickAgent = (id: string) => {
    const step = ownOf(id)
    setBackend(id)
    setPicked(step ? step.loginProfile ?? null : undefined)
    setModel(step?.model ?? null)
    setModelOptions(step?.modelOptions ?? {})
  }
  const save = () => onSave({ ...own, backend, loginProfile, model, modelOptions })
  return (
    <SheetPage>
      <ModalHeader title={tr(initial ? `Edit fallback` : `Add fallback`)} trailing={<IconButton title={initial ? 'Save' : 'Add'} icon="checkmark" disabled={!backend} onClick={save} />} />
      <CardForm>
        <CardSection>
          <AgentModelPicker backend={backend} models={models} selection={model} onChange={pickModel} options={modelOptions} onOptionsChange={setModelOptions} sessionNote={false} agents={choices.map(agentChoice)} onAgentChange={pickAgent} collapsible={false} />
          {agent?.loginProfiles ? <LoginProfileRow backend={agent} loginProfile={loginProfile} free={free} onChange={setPicked} /> : null}
        </CardSection>
      </CardForm>
    </SheetPage>
  )
}
