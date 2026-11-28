import type { BotDraft } from '@shared/models'

/** The network state that decides which bot settings are shown. */
export type NetworkState = { joined?: boolean | null; assistants?: boolean | null } | null | undefined

/** One network switch's presentation and the draft key it writes. */
export type NetworkSwitch = {
  key: 'offeredToNetwork' | 'reachableByAssistants'
  title: string
  detail: string
  on: boolean
  field: 'offeredToNetwork' | 'reachableByAssistants'
}

/** The switches a host can show for `draft`, in settings order. Each becomes its own `CardSection` row, so an
 * empty list leaves no row and a visible one always gets its own hairline. */
export function visibleNetworkSwitches(draft: BotDraft, network: NetworkState): NetworkSwitch[] {
  const rows: NetworkSwitch[] = []
  if (draft.offeredToNetwork !== undefined && network?.joined) {
    rows.push({
      key: 'offeredToNetwork',
      title: 'Offered to the network',
      detail: 'Bots on your other computers can see and message this bot.',
      on: draft.offeredToNetwork ?? false,
      field: 'offeredToNetwork',
    })
  }
  if (draft.reachableByAssistants !== undefined && network?.assistants) {
    rows.push({
      key: 'reachableByAssistants',
      title: 'Reachable by cloud assistants',
      detail: 'ChatGPT or Claude in your Codync account can see and message this bot.',
      on: draft.reachableByAssistants ?? false,
      field: 'reachableByAssistants',
    })
  }
  return rows
}
