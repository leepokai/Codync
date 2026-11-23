import { restLine, restOf, withThreadBackend } from '@shared/fallbacks'
import type { Bot, BotDraft, Fallback } from '@shared/models'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { ModelPicker } from '../chat/RunOptions'
import { loginSeats } from '../chat/seats'
import { useAgentModels } from './agent-models'

/** Advisor mode's agent for the bot's threads: its login and model picked in the model panel, as the bot's own. */
export function ThreadsAgentRow({ agent, bot, onChange }: { agent: Fallback; bot: Bot; onChange: (update: (d: BotDraft) => BotDraft) => void }) {
  const tr = useTr()
  const store = useStore()
  const models = useAgentModels(agent.backend, agent.loginProfile)
  const backends = store.hello?.backends ?? []
  const set = (change: Partial<Fallback>) => onChange((d) => (d.threadAgent ? { ...d, threadAgent: { ...d.threadAgent, ...change } } : d))
  const seats = loginSeats({
    backends, on: agent, tr,
    pick: (b, loginProfile) => onChange((d) => {
      const next = withThreadBackend(d, b.id)
      return next.threadAgent ? { ...next, threadAgent: { ...next.threadAgent, loginProfile } } : next
    }),
  })
  const rest = restOf(bot, agent, Date.now())
  return (
    <VStack gap={8} className="fade-in">
      <Text tone="text">{tr("Threads' agent")}</Text>
      <ModelPicker
        trigger="row"
        backend={agent.backend}
        models={models}
        model={{ id: agent.model ?? null, options: agent.modelOptions }}
        set={(m) => set({ model: m.id, modelOptions: m.options })}
        seats={seats}
        adds={backends.find((b) => b.id === agent.backend)?.loginProfiles ? 'login' : 'agent'}
      >
        {rest ? <Text size="caption" tone="secondary">{tr(restLine(rest, Date.now()))}</Text> : null}
      </ModelPicker>
    </VStack>
  )
}
