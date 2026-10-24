import type { HostClient } from '../client/host-client'
import { Observable } from '../lib/observable'

/** One memory entry (the host's `codync_mem::store::Entry`). */
export interface MemoryEntry {
  id: number
  /** The bot whose own layer holds it; null for what all bots share. */
  bot: string | null
  state: 'active' | 'archived' | string
  text: string
  /** user · bot · dream · imported */
  author: string
  authorBot: string | null
  pinned: boolean
  evidence: string[]
  createdAt: number
  updatedAt: number
}

export interface MemoryLayer { entries: MemoryEntry[]; used: number; cap: number }

/** One dreaming run: what it added and archived (`undone` when an older app undid it). */
export interface DreamRun {
  id: number
  at: number
  undone: boolean
  added: { id: number; text: string }[]
  archived: { id: number; text: string }[]
  /** Long-form files it added a line to. */
  files?: { name: string; shared: boolean; version: number }[]
}

/** A long-form memory file as the host last recorded it (`codync_mem::files::File`). */
export interface MemoryFile {
  bot: string | null
  name: string
  description: string | null
  tokens: number
  deleted: boolean
  updatedAt: number
}

export interface MemoryFiles {
  yours: { files: MemoryFile[]; folder: string }
  bot: { files: MemoryFile[]; folder: string }
  /** Tokens one file may hold, and files one folder may hold. */
  cap: number
  folderCap: number
}

/** A file as it is on disk now. */
export interface MemoryFileDetail {
  file: string
  shared: boolean
  content: string | null
  path: string
}

export interface MemoryView {
  /** Whether this bot uses what all bots share. */
  shared: boolean
  yours: MemoryLayer
  bot: MemoryLayer
  dreams: DreamRun[]
  /** Claude Code's own notes for the bot's project folder, read-only. */
  projectMemory: { path: string; text: string } | null
  memoryRev: number
  /** The long-form files of both layers. */
  files?: MemoryFiles
}

const OLDER_HOST = "Update Codync on your computer to see this bot's memory."

/** One bot's memory, read-only: the bot changes it. */
export class MemoryStore extends Observable {
  view: MemoryView | null = null
  error: string | null = null
  private request = 0

  constructor(readonly botId: string, private readonly client: () => Promise<HostClient>) { super() }

  async load() {
    const request = ++this.request
    try {
      const view = await (await this.client()).call<MemoryView>('memory', { botId: this.botId })
      // A host before 2.18.0 answers with its old memory listing, which has no layers or dreams.
      if (!Array.isArray(view.dreams)) throw new Error(OLDER_HOST)
      if (request === this.request) {
        this.view = view
        this.error = null
      }
    } catch (error) {
      if (request === this.request) this.error = message(error)
    }
    this.changed()
  }

  /** A long-form file as it is now. */
  async file(name: string, shared: boolean): Promise<MemoryFileDetail | null> {
    try {
      return await (await this.client()).call<MemoryFileDetail>('memoryFile', { botId: this.botId, file: name, shared })
    } catch (error) {
      this.error = message(error)
      this.changed()
      return null
    }
  }
}

/** Who wrote an entry, for its detail line. */
export function authorLabel(entry: MemoryEntry, botName: (id: string | null) => string) {
  switch (entry.author) {
    case 'user': return 'You wrote it'
    case 'bot': return `${botName(entry.authorBot)} remembered it`
    case 'dream': return 'Dreaming'
    default: return 'Moved from the old memory'
  }
}

function message(error: unknown) { return error instanceof Error ? error.message : String(error) }
