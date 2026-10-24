import { readArtifactHTML, type Artifact, type LibraryItem } from '@shared/artifacts'
import { FILE_CHUNK_SIZE } from '../../shared/files.ts'
import type { HostClient } from '../client/host-client'

/** An artifact's HTML as last read; `updatedAt` and `sha256` name the version it is. */
export interface ArtifactHTML { updatedAt: number; sha256: string; bytes: Uint8Array }

/**
 * One computer's artifacts (`host/src/chat/artifacts.rs`): their details by id, from `library`, `readArtifact` and the `artifact`
 * events, and the HTML of each one shown, read again once a newer version is published.
 */
export class Artifacts {
  readonly byId = new Map<string, Artifact>()
  readonly html = new Map<string, ArtifactHTML>()
  readonly failed = new Map<string, string>()
  private reading = new Map<string, Promise<ArtifactHTML>>()
  private changed: () => void
  constructor(changed: () => void) { this.changed = changed }

  upsert(artifact: Artifact) {
    const known = this.byId.get(artifact.id)
    if (known && known.updatedAt > artifact.updatedAt) return
    this.byId.set(artifact.id, artifact)
    this.changed()
  }

  /** The chat's Library (`library`); its artifacts join the ones known here. */
  async library(client: HostClient | null, botId: string): Promise<LibraryItem[]> {
    if (!client) throw new Error('Connect to this computer to see its library')
    const { items } = await client.library(botId)
    for (const item of items) if (item.kind === 'artifact') this.upsert(item.artifact)
    return items
  }

  /** Reads the artifact's HTML in the background unless the newest version known is already here; a failure shows. */
  load(client: HostClient | null, botId: string, id: string, updatedAt = 0) {
    if (this.current(id, updatedAt) || this.reading.has(id) || !client) return
    this.failed.delete(id)
    this.fetch(client, botId, id).catch((error: unknown) => {
      this.failed.set(id, error instanceof Error ? error.message : String(error))
      this.changed()
    })
  }

  async rename(client: HostClient | null, botId: string, id: string, title: string) {
    if (!client) throw new Error('Connect to this computer to rename the artifact')
    this.upsert((await client.renameArtifact(botId, id, title)).artifact)
  }

  /** Saves the artifact's newest HTML where the user picks (the native Save dialog). */
  async save(client: HostClient | null, artifact: Artifact) {
    const html = await this.newest(client, artifact)
    const title = this.byId.get(artifact.id)?.title ?? artifact.title
    const name = `${title.replace(/[/\\\x00-\x1f\x7f]/g, '-').slice(0, 190)}.html`
    const handle = await window.codync.files.begin({ id: artifact.id, name, size: html.bytes.length, sha256: html.sha256 })
    if (!handle) return
    try {
      for (let offset = 0; offset < html.bytes.length; offset += FILE_CHUNK_SIZE) {
        await window.codync.files.write(handle, offset, html.bytes.subarray(offset, offset + FILE_CHUNK_SIZE))
      }
      await window.codync.files.finish(handle)
    } finally {
      await window.codync.files.cancel(handle).catch(console.error)
    }
  }

  /** Opens the artifact's newest HTML in this computer's browser. */
  async openInBrowser(client: HostClient | null, artifact: Artifact) {
    await window.codync.artifacts.openInBrowser(artifact.id, (await this.newest(client, artifact)).bytes)
  }

  /** The artifact's newest HTML, read when it isn't here yet. */
  private async newest(client: HostClient | null, artifact: Artifact): Promise<ArtifactHTML> {
    const cached = this.html.get(artifact.id)
    if (cached && this.current(artifact.id)) return cached
    if (!client) throw new Error('Connect to this computer to open the artifact')
    return this.fetch(client, artifact.botId, artifact.id)
  }

  private current(id: string, updatedAt = 0) {
    const newest = Math.max(updatedAt, this.byId.get(id)?.updatedAt ?? 0)
    return (this.html.get(id)?.updatedAt ?? -1) >= newest
  }

  /** One read of an artifact at a time; callers share it. */
  private fetch(client: HostClient, botId: string, id: string): Promise<ArtifactHTML> {
    const running = this.reading.get(id)
    if (running) return running
    const read = this.read(client, botId, id).finally(() => {
      this.reading.delete(id)
      this.changed()
    })
    this.reading.set(id, read)
    return read
  }

  private async read(client: HostClient, botId: string, id: string): Promise<ArtifactHTML> {
    const { artifact, bytes } = await readArtifactHTML((offset) => client.readArtifact(botId, id, offset))
    const read = { updatedAt: artifact.updatedAt, sha256: artifact.sha256, bytes }
    this.html.set(id, read)
    this.upsert(artifact)
    // A newer version published while this one was read is read next.
    if (!this.current(id)) setTimeout(() => this.load(client, botId, id))
    return read
  }
}
