/** Codync private relay. Listens on loopback; a reverse proxy terminates TLS in front of it. */

import { resolve } from 'node:path'
import { RelayStore } from './store.ts'
import { startRelay } from './http.ts'

const dataDir = process.env.CODYNC_PRIVATE_RELAY_DATA ?? '/var/lib/codync-private-relay'
const port = Number(process.env.PORT ?? 8788)
const host = '127.0.0.1'
const token = process.env.CODYNC_PRIVATE_RELAY_TOKEN ?? ''
if (token.length < 16 || token.length > 256 || !/^[\x21-\x7E]+$/.test(token)) {
  console.error('CODYNC_PRIVATE_RELAY_TOKEN must be 16 to 256 visible ASCII characters.')
  process.exit(1)
}
// What computers put in their Relay address. Unset, only this loopback address verifies.
const publicUrl = process.env.CODYNC_PRIVATE_RELAY_PUBLIC_URL?.trim() || undefined

const store = new RelayStore(resolve(dataDir, 'relay.sqlite'))
const relay = await startRelay({ store, host, port, token, publicUrl })
console.log(`codync private relay listening on ${relay.url} for ${publicUrl ?? relay.url}`)
