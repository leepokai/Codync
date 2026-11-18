import type { CloudComputer } from '../store/cloud'

/** The one action an account computer's row offers; `opening`: Open, held until its bots are there to open. */
export type AccountComputerAction = 'open' | 'opening' | 'cancel' | 'ask' | null

/**
 * `store`: this app's connection to the computer once it is approved there, with the bots it has loaded so far.
 * Open goes to its first bot, so until one arrives it would only close Settings.
 */
export function accountComputerAction(computer: CloudComputer, olderCopy: boolean, store: { isLoopback: boolean; roster: unknown[] } | null, asking: boolean): AccountComputerAction {
  if (store && !store.isLoopback) return store.roster.length ? 'open' : 'opening'
  if (asking) return 'cancel'
  return computer.online && !computer.cloud && !olderCopy && computer.access !== 'granted' ? 'ask' : null
}
