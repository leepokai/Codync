import type { HostClient } from '../../client/host-client'

/** Model preferences follow the user; optional Live support belongs to the currently connected host. */
export async function requireLiveHost(client: Pick<HostClient, 'call'>) {
  const status = await client.call<{ supportsLive?: boolean }>('voiceStatus', {})
  if (status.supportsLive !== true) throw new Error('Update Codync on the computer to use GPT-Live.')
}
