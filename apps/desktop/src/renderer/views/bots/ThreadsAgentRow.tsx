import { restLine, restOf, withThreadBackend } from '@shared/fallbacks'
import type { Bot, BotDraft, Fallback } from '@shared/models'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { useAgentModels } from './agent-models'
import { AgentModelPicker } from './AgentModelPicker'
import { agentChoice } from './Fallbacks'
import { LoginProfileRow } from './LoginProfileRow'

/** Advisor mode's agent for the bot's threads: picked in place with its model and login, as the bot's own. */
export function ThreadsAgentRow({ agent, bot, onChange }: { agent: Fallback; bot: Bot; onChange: (update: (d: BotDraft) => BotDraft) => void }) {
  const tr = useTr()
  const store = useStore()
  const models = useAgentModels(agent.backend)
  const backends = store.hello?.backends ?? []
  const backend = backends.find((b) => b.id === agent.backend)
  const set = (change: Partial<Fallback>) => onChange((d) => (d.threadAgent ? { ...d, threadAgent: { ...d.threadAgent, ...change } } : d))
  const rest = restOf(bot, agent, Date.now())
  return (
    <VStack gap={8} className="fade-in">
      <Text tone="text">{tr("Threads' agent")}</Text>
      <AgentModelPicker
        backend={agent.backend}
        models={models}
        selection={agent.model}
        onChange={(model, modelOptions) => set({ model, modelOptions })}
        options={agent.modelOptions}
        sessionNote={false}
        agents={backends.map(agentChoice)}
        onAgentChange={(id) => onChange((d) => withThreadBackend(d, id))}
      >
        {rest ? <Text size="caption" tone="secondary">{tr(restLine(rest, Date.now()))}</Text> : null}
        {backend?.loginProfiles ? <LoginProfileRow backend={backend} loginProfile={agent.loginProfile ?? null} onChange={(loginProfile) => set({ loginProfile })} /> : null}
      </AgentModelPicker>
    </VStack>
  )
}
