import type { BotDraft } from '@shared/models'
import { useStore } from '../../store/context'
import { SwitchRow } from './parts'

/**
 * Who beyond this computer reaches the bot: the user's other computers (while this one is in a
 * network) and the cloud assistant (while it's on). Each shows only from a host that has it.
 */
export function NetworkSwitches({ draft, set }: { draft: BotDraft; set: <K extends keyof BotDraft>(key: K, value: BotDraft[K]) => void }) {
  const network = useStore().network
  return (
    <>
      {draft.offeredToNetwork !== undefined && network?.joined ? (
        <SwitchRow
          title="Offered to the network"
          detail="Bots on your other computers can see and message this bot."
          on={draft.offeredToNetwork}
          onChange={(on) => set('offeredToNetwork', on)}
        />
      ) : null}
      {draft.reachableByAssistants !== undefined && network?.assistants ? (
        <SwitchRow
          title="Reachable by cloud assistants"
          detail="ChatGPT or Claude in your Codync account can see and message this bot."
          on={draft.reachableByAssistants}
          onChange={(on) => set('reachableByAssistants', on)}
        />
      ) : null}
    </>
  )
}
