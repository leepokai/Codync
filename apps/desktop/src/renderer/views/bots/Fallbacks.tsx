import { useState } from 'react'
import { MAX_FALLBACKS, moveFallback, restLine, restOf, runningAgent, sameLogin, stepAgents, stepBannerPhrase, stepLogin, stepLogins, stepOn, type Login } from '@shared/fallbacks'
import type { Backend, Bot, BotDraft, Fallback } from '@shared/models'
import { AgentIcon } from '../../components/AgentIcon'
import { Button, DropdownMenu, IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog } from '../../components/Overlay'
import { HStack, IconSlot, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { fill } from '@shared/fill'
import { useStore } from '../../store/context'
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

/** The Agent card's Fallbacks: the agents that carry on in order, each picked in place like the bot's own, added with +,
 * moved and removed from its menu, saved with the form. `threads`: Advisor mode's, after the threads' own agent. */
export function FallbacksRow({ draft, bot, onChange, threads = false }: { draft: BotDraft; bot: Bot; onChange: (update: (d: BotDraft) => BotDraft) => void; threads?: boolean }) {
  const store = useStore()
  const tr = useTr()
  const [about, setAbout] = useState(false)
  // The step + just added: it opens on its pickers.
  const [added, setAdded] = useState<number | null>(null)
  // Bumped when a move or removal renumbers the rows, so each starts afresh on the step now in its place.
  const [order, setOrder] = useState(0)
  const key = threads ? 'threadFallbacks' : 'fallbacks'
  const list = draft[key] ?? []
  const head: Login = (threads ? draft.threadAgent : null) ?? draft
  const backends = store.hello?.backends ?? []
  // Each login is in the chain once, its first agent's included: an agent comes again only on another of its login profiles.
  const takenBut = (index: number): Login[] => [head, ...list.filter((_, j) => j !== index)]
  const choices = stepAgents(backends, takenBut(list.length))
  const update = (change: (list: Fallback[]) => Fallback[]) => onChange((d) => ({ ...d, [key]: change(d[key] ?? []) }))
  // The chat's step; Advisor mode's threads each run their own.
  const step = threads ? null : stepLogin(bot)
  const add = () => {
    const agent = choices.find((b) => b.available) ?? choices[0]
    if (!agent) return
    setAdded(list.length)
    update((l) => [...l, stepOn(agent, takenBut(list.length))])
  }
  const renumber = (change: (list: Fallback[]) => Fallback[]) => {
    setAdded(null)
    setOrder((o) => o + 1)
    update(change)
  }
  const now = Date.now()
  return (
    <VStack gap={8} className={threads ? 'fade-in' : undefined}>
      <HStack gap={2}>
        <Text tone="text">{tr(threads ? "Threads' fallbacks" : 'Fallbacks')}</Text>
        <IconButton title="About fallbacks" icon="info.circle" size={22} onClick={() => setAbout(true)} />
        <Spacer />
        {list.length < MAX_FALLBACKS && choices.length ? <IconButton title="Add fallback" icon="plus" className="fade-in" onClick={add} /> : null}
      </HStack>
      {list.length ? (
        // By place, not login: picking another agent keeps the row (and its open picker) where it is.
        list.map((f, i) => (
          <FallbackRow
            key={`${order}:${i}`}
            fallback={f}
            index={i}
            count={list.length}
            bot={bot}
            inUse={!!step && sameLogin(step, f)}
            now={now}
            backends={backends}
            taken={takenBut(i)}
            expanded={added === i}
            onChange={(next) => update((l) => l.toSpliced(i, 1, next))}
            move={(by) => renumber((l) => moveFallback(l, i, by))}
            remove={() => renumber((l) => l.filter((_, j) => j !== i))}
          />
        ))
      ) : (
        <Text tone="secondary">{tr(`No fallbacks yet.`)}</Text>
      )}
      <Text size="caption" tone="secondary">
        {tr(threads
          ? "If {agent} runs out of usage or can't answer, threads carry on with the next one."
          : 'If {agent} runs out of usage or can\'t answer, the bot carries on with the next one. Chat, memory and workspace carry over.').replace('{agent}', store.backendName(head.backend))}
      </Text>
      <Dialog open={about} title={tr('Fallbacks')} message={tr(ABOUT)} actions={[]} cancel="OK" onClose={() => setAbout(false)} />
    </VStack>
  )
}

/** One fallback: its place, its agent, model and login picked in place, whether it's resting or in use, and its menu. */
function FallbackRow({ fallback, index, count, bot, inUse, now, backends, taken, expanded, onChange, move, remove }: {
  fallback: Fallback
  index: number
  count: number
  bot: Bot
  /** The bot runs on it right now. */
  inUse: boolean
  now: number
  backends: Backend[]
  /** The rest of the chain's logins. */
  taken: Login[]
  expanded: boolean
  onChange: (fallback: Fallback) => void
  move: (by: number) => void
  remove: () => void
}) {
  const tr = useTr()
  const models = useAgentModels(fallback.backend)
  // The step as it was on its own agent while another is picked: picking that agent again brings back its model and login.
  const [own, setOwn] = useState<Fallback | null>(null)
  const choices = stepAgents(backends, taken, [fallback, own])
  const agent = choices.find((b) => b.id === fallback.backend)
  const loginProfile = fallback.loginProfile ?? null
  const profile = loginProfile ? agent?.loginProfiles?.find((p) => p.id === loginProfile) : null
  // The step's login shows beside its agent: "Claude Code (Work)".
  const agents = choices.map((b) => {
    const choice = agentChoice(b)
    return b.id === fallback.backend && profile ? { ...choice, label: `${choice.label} (${profile.name ?? tr('New login profile')})` } : choice
  })
  const rest = restOf(bot, fallback, now)
  return (
    <VStack gap={2} className="fade-in">
      <HStack gap={8} align="flex-start">
        <IconSlot width={12} tone="secondary" className="fallback-number">
          <Text tabular>{index + 1}</Text>
        </IconSlot>
        <VStack grow>
          <AgentModelPicker
            backend={fallback.backend}
            models={models}
            selection={fallback.model}
            onChange={(model, modelOptions) => onChange({ ...fallback, model, modelOptions })}
            options={fallback.modelOptions}
            sessionNote={false}
            agents={agents}
            onAgentChange={(id) => {
              const next = choices.find((b) => b.id === id)
              if (!next) return
              const step = stepOn(next, taken, own)
              setOwn(step === own ? null : own ?? fallback)
              onChange(step)
            }}
            expanded={expanded}
          >
            {agent?.loginProfiles ? <LoginProfileRow backend={agent} loginProfile={loginProfile} free={stepLogins(agent, fallback, taken)} onChange={(id) => onChange({ ...fallback, loginProfile: id })} /> : null}
          </AgentModelPicker>
        </VStack>
        {inUse ? <Text size="caption" weight="medium" tone="text" className="fallback-chip fade-in">{tr(`In use`)}</Text> : null}
        <DropdownMenu
          className="icon-button fallback-menu"
          title={tr('More')}
          items={() => [
            ...(index > 0 ? [{ title: 'Move up', icon: 'arrow.up', action: () => move(-1) }] : []),
            ...(index < count - 1 ? [{ title: 'Move down', icon: 'arrow.down', action: () => move(1) }] : []),
            { title: 'Remove', icon: 'trash', destructive: true, divider: true, action: remove },
          ]}
        >
          <Icon name="ellipsis" size={14} weight="medium" />
        </DropdownMenu>
      </HStack>
      {rest ? <Text size="caption" tone="secondary" className="fallback-rest">{tr(restLine(rest, now))}</Text> : null}
    </VStack>
  )
}
