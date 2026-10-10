import type { HostClient } from '../client/host-client'
import { Observable } from '../lib/observable'

export interface MemoryFact {
  id: string
  title: string
  content: string
  createdAt: number
  kind: string
  memoryType: string
  scope: string
  topicKey: string | null
  pinned: boolean
  reviewAfter: string | null
  revisionCount: number
  source: string | null
}

export interface MemoryDraft {
  id?: string
  title: string
  content: string
  scope: string
  memoryType: string
  topicKey: string
}

interface Listing { facts: MemoryFact[]; total: number; nextOffset: number | null }
export interface MemoryDetail {
  fact: MemoryFact
  history: { result: string; history?: { version: number; title: string; content: string; created_at: string }[]; history_cursor?: number }
  timeline: { result: string }
}

/** Owns requests, errors and stale-response protection for one bot's memory. */
export class MemoryStore extends Observable {
  facts: MemoryFact[] = []
  query = ''
  filter = 'all'
  total = 0
  nextOffset: number | null = null
  loaded = false
  busy = false
  error: string | null = null
  detail: MemoryDetail | null = null
  backup = ''
  private request = 0
  private detailRequest = 0
  private searchTimer?: ReturnType<typeof setTimeout>

  constructor(readonly botId: string, private readonly client: () => Promise<HostClient>) { super() }

  async load(more = false) {
    const request = ++this.request
    const offset = more ? this.nextOffset : 0
    if (offset === null) return
    this.error = null
    this.changed()
    try {
      const listing = await (await this.client()).call<Listing>('memory', {
        botId: this.botId, query: this.query, filter: this.filter, offset,
      }, 240_000)
      if (request !== this.request) return
      this.facts = more ? [...this.facts, ...listing.facts] : listing.facts
      this.total = listing.total
      this.nextOffset = listing.nextOffset
      this.loaded = true
    } catch (error) {
      if (request === this.request) this.error = message(error)
    }
    this.changed()
  }

  search(query: string, filter = this.filter) {
    this.query = query
    this.filter = filter
    ++this.request
    // The search field shows `query`: re-render now, not when the debounced load lands.
    this.changed()
    clearTimeout(this.searchTimer)
    this.searchTimer = setTimeout(() => { void this.load() }, 250)
  }

  async mutate(method: string, body: object = {}) {
    if (this.busy) return false
    return this.perform(async () => {
      await (await this.client()).call(method, { botId: this.botId, ...body }, 240_000)
    })
  }

  private async perform(work: () => Promise<void>) {
    if (this.busy) return false
    this.busy = true
    this.error = null
    this.changed()
    try {
      await work()
      await this.load()
      return true
    } catch (error) {
      this.error = message(error)
      return false
    } finally {
      this.busy = false
      this.changed()
    }
  }

  async inspect(fact: MemoryFact, cursor?: number) {
    const request = ++this.detailRequest
    this.error = null
    if (!cursor) this.detail = null
    this.changed()
    try {
      const detail = await (await this.client()).call<MemoryDetail>('memoryDetail', {
        botId: this.botId, id: fact.id, ...(cursor ? { historyCursor: cursor } : {}),
      }, 240_000)
      if (request === this.detailRequest) this.detail = detail
    } catch (error) { if (request === this.detailRequest) this.error = message(error) }
    this.changed()
  }

  async export() {
    this.error = null
    this.backup = ''
    this.changed()
    try {
      const client = await this.client()
      const result = await client.call<{ json?: string; uploadId?: string }>('exportMemory', { botId: this.botId }, 240_000)
      this.backup = result.uploadId ? new TextDecoder('utf-8', { fatal: true }).decode(await client.readUpload(this.botId, result.uploadId)) : result.json ?? ''
    } catch (error) { this.error = message(error) }
    this.changed()
  }

  async import(text: string) {
    try { JSON.parse(text) }
    catch { this.error = 'Enter a valid Engram JSON backup.'; this.changed(); return false }
    return this.perform(async () => {
      const data = new TextEncoder().encode(text)
      const client = await this.client()
      if (data.length <= 384 * 1024) {
        await client.call('importMemory', { botId: this.botId, json: text }, 240_000)
      } else {
        const uploadId = crypto.randomUUID()
        await client.upload(this.botId, uploadId, 'engram-backup.json', data)
        await client.call('importMemory', { botId: this.botId, uploadId }, 240_000)
      }
    })
  }
}

function message(error: unknown) { return error instanceof Error ? error.message : String(error) }
