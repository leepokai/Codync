import type { BotStore } from '../../store/bot-store'

/** Opens (or brings forward) the Remote screen window of a computer this app manages. */
export function openRemoteScreen(store: BotStore) {
  window.codync.screenViewer.open(store.computer.id, store.hostName)
}
