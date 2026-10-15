import type { SSHAttachment } from '@shared/ssh'
import type { BotStore } from '../../store/bot-store'

/** Opens the VNC viewer for a computer reached Over SSH. */
export function openComputerScreen(store: BotStore, attachment: SSHAttachment) {
  void window.codync.screen.open({
    computerId: store.computer.id,
    name: store.hostName || attachment.computer.name,
    vncPort: attachment.vncPort,
  }).catch((error: unknown) => {
    store.setError(error instanceof Error ? error.message : "Can't open the screen from here. Connect Over SSH and try again.")
  })
}
