import { app, ipcMain, type WebContents } from 'electron'
import { appHeader } from '../../shared/compat'
import type { AccountService } from '../account'
import { isAttended, onAttention } from '../attention'
import { identity, signatureHeader } from '../cloud'
import { unb64url } from './crypto.ts'
import { RelayChannel, unreachable, type ChannelError, type Socket } from './relay-channel.ts'

// The renderer's way to account computers over the relay: the same calls and streams as the loopback proxy
// (`host-proxy.ts`), carried by one `RelayChannel` per computer.

const channels = new Map<string, RelayChannel>()
const streams = new Map<string, () => void>()

function closeAll() {
  for (const stop of streams.values()) stop()
  streams.clear()
  for (const channel of channels.values()) channel.close()
  channels.clear()
}

async function channelFor(account: AccountService, computerId: string, signKey: string): Promise<RelayChannel> {
  const base = account.state.cloudURL
  const user = account.state.user
  if (!base || !user) throw { status: 0, message: 'Sign in again to reach your account.' } satisfies ChannelError
  const key = `${user.userId}/${computerId}`
  const existing = channels.get(key)
  if (existing?.signKey === signKey) return existing
  existing?.close()
  const id = await identity(user.userId)
  const channel = new RelayChannel({
    computerId,
    signKey,
    device: { key: id.key, pub: unb64url(id.publicKey) },
    dial: () => {
      const url = new URL(`v1/relay/device/${computerId}?v=1`, `${base}/`)
      const headers = { 'Codync-Sig': signatureHeader(id, 'GET', url, Buffer.alloc(0)), 'Codync-App': appHeader(app.getVersion()) }
      url.protocol = url.protocol === 'http:' ? 'ws:' : 'wss:'
      // Node's WebSocket (undici) takes request headers in its init object.
      const socket = new WebSocket(url, { headers } as unknown as string[])
      socket.addEventListener('close', (e) => e.code !== 1000 && console.warn(`relay channel to ${computerId} closed (${e.code} ${e.reason})`))
      return socket as unknown as Socket
    },
    onRefused: () => channels.get(key) === channel && channels.delete(key),
  })
  channel.setAttended(isAttended())
  channels.set(key, channel)
  return channel
}

/** `/events?since&client` and `/term/{id}`, the loopback stream paths, as channel subscriptions (§6.5). */
function subscription(path: string): { sub: 'events' | 'term'; body: unknown } | null {
  const url = new URL(path, 'http://host')
  if (url.pathname === '/events') return { sub: 'events', body: { since: Number(url.searchParams.get('since') ?? 0), client: url.searchParams.get('client') ?? 'desktop' } }
  const term = /^\/term\/([^/]+)$/.exec(url.pathname)
  return term ? { sub: 'term', body: { term: decodeURIComponent(term[1]!) } } : null
}

function openStream(account: AccountService, sender: WebContents, id: string, computerId: string, signKey: string, path: string) {
  const send = (channel: string, payload: unknown) => {
    if (!sender.isDestroyed()) sender.send(channel, id, payload)
  }
  const end = (error: ChannelError | null) => {
    if (!streams.delete(id)) return
    send('host:stream:end', error)
  }
  const sub = subscription(path)
  if (!sub) return send('host:stream:end', { status: 404, message: 'Not found' })
  let stop: (() => void) | null = null
  streams.set(id, () => stop?.())
  channelFor(account, computerId, signKey).then((channel) => {
    if (!streams.has(id)) return
    stop = channel.subscribe(sub.sub, sub.body, (event) => send('host:stream:data', JSON.stringify(event)), end)
  }, (error: ChannelError) => end(error))
}

export function registerChannels(account: AccountService) {
  ipcMain.handle('channel:call', async (_e, computerId: string, signKey: string, method: string, body: unknown, timeoutMs: number) => {
    try {
      const channel = await channelFor(account, computerId, signKey)
      return { ok: true, value: await channel.call(method, body, timeoutMs) }
    } catch (error) {
      const e = error as Partial<ChannelError>
      return { ok: false, error: typeof e?.status === 'number' ? e : unreachable() }
    }
  })
  ipcMain.on('channel:stream:open', (e, id: string, computerId: string, signKey: string, path: string) => openStream(account, e.sender, id, computerId, signKey, path))
  ipcMain.on('channel:stream:close', (_e, id: string) => {
    const stop = streams.get(id)
    streams.delete(id)
    stop?.()
  })
  onAttention((on) => {
    for (const channel of channels.values()) channel.setAttended(on)
  })
  // Another account (or none): this one's channels go.
  let user = account.state.user?.userId ?? null
  account.on('change', () => {
    const now = account.state.user?.userId ?? null
    if (now === user) return
    user = now
    closeAll()
  })
}
