import type { ReactNode } from 'react'
import type { BotDraft } from '@shared/models'
import { SwitchRow } from './parts'
import { visibleNetworkSwitches, type NetworkState } from './network-switches'

export type { NetworkState } from './network-switches'

/** Switch rows beyond this computer, returned as separate children so `CardSection` gives each its own row and
 * hairline; none where the host can't show them, so no empty row is left over. Layout only: a missing switch is an
 * empty list, not a failed load, so there is no content-failure event to report. */
export function networkSwitches(draft: BotDraft, network: NetworkState, set: <K extends keyof BotDraft>(key: K, value: BotDraft[K]) => void): ReactNode[] {
  return visibleNetworkSwitches(draft, network).map(({ key, title, detail, on, field }) => (
    <SwitchRow key={key} title={title} detail={detail} on={on} onChange={(value) => set(field, value)} />
  ))
}
