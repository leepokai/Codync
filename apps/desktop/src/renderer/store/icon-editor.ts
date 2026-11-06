import { account } from './account'
import type { CustomIcons } from '@shared/custom-icons'
import { Observable } from '../lib/observable'
import type { BotStore } from './bot-store'

export class IconEditor extends Observable {
  busy = false
  error: string | null = null

  constructor(private store: BotStore, readonly target: string) { super() }

  async save(change: { file?: File; url?: string; tinted?: boolean; remove?: boolean }) {
    if (this.busy) return
    this.busy = true
    this.error = null
    this.changed()
    try {
      if ((change.file || change.url) && !account.isSignedIn) throw new Error('Sign in to use custom images.')
      const epoch = this.store.iconsEpoch
      const client = this.store.client
      if (!client || this.store.connection.kind !== 'online') throw new Error('Connect to your computer to change its icons.')
      const { file, ...body } = change
      const data = file ? await readImage(file) : undefined
      const result = await client.call<{ icons: CustomIcons; iconsRevision: number }>('setIcon', { target: this.target, ...body, data }, 60_000)
      if (epoch === this.store.iconsEpoch) this.store.applyIcons(result.icons, result.iconsRevision)
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    } finally {
      this.busy = false
      this.changed()
    }
  }
}

async function readImage(file: File): Promise<string> {
  if (!file.size || file.size > 5 * 1024 * 1024) throw new Error('Choose an image up to 5 MiB.')
  const bytes = new Uint8Array(await file.arrayBuffer())
  let binary = ''
  for (let at = 0; at < bytes.length; at += 8192) binary += String.fromCharCode(...bytes.subarray(at, at + 8192))
  return btoa(binary)
}
