import { Observable } from '../lib/observable.ts'
import type { BotStore } from './bot-store'

/** What an export file holds, as the host reads it (`previewImport`). */
export interface ImportPlan {
  source: string
  bots: { key: string; name: string; description: string; skills: string[]; routines: unknown[]; memories: string[]; files: string | null }[]
  groups: { key: string; name: string; members: string[] }[]
  warnings: string[]
}

export interface ImportResult {
  bots: { key: string; id: string; name: string; skills: number; routines: number; memories: number; files: number }[]
  groups: { key: string; id: string; name: string }[]
  warnings: string[]
}

export type ImportStep =
  | { kind: 'start' }
  | { kind: 'review'; path: string; plan: ImportPlan; skipped: string[] }
  | { kind: 'done'; result: ImportResult }

/** Bringing bots over from another app's export file on this computer (the host's `import`). */
export class BotImport extends Observable {
  step: ImportStep = { kind: 'start' }
  /** Bots and groups left out by the user (a bot and a group may share a key). */
  off = { bot: new Set<string>(), group: new Set<string>() }
  busy = false
  copied = false
  error: string | null = null

  private store: BotStore

  constructor(store: BotStore) {
    super()
    this.store = store
  }

  /** The skill that writes the export in Grok Bot, on the clipboard. */
  async copySkill() {
    await this.run(async () => {
      const { text } = await (await this.store.ready()).call<{ text: string }>('importSkill', {})
      window.codync.app.copy(text)
      this.copied = true
    })
  }

  async choose() {
    const path = await window.codync.app.pickExport()
    if (!path) return
    await this.run(async () => {
      const read = await (await this.store.ready()).call<{ plan: ImportPlan; skipped: string[] }>('previewImport', { path }, 120_000)
      this.off = { bot: new Set(), group: new Set() }
      this.step = { kind: 'review', path, ...read }
    })
  }

  picked(kind: 'bot' | 'group', key: string) {
    return !this.off[kind].has(key)
  }

  toggle(kind: 'bot' | 'group', key: string) {
    const off = this.off[kind]
    if (off.has(key)) off.delete(key)
    else off.add(key)
    this.changed()
  }

  /** A group can come along only with one of its bots. */
  groupAvailable(members: string[]) {
    return members.some((key) => this.picked('bot', key))
  }

  get canImport() {
    return this.step.kind === 'review' && this.step.plan.bots.some((b) => this.picked('bot', b.key)) && !this.busy
  }

  async importPicked() {
    const step = this.step
    if (step.kind !== 'review' || !this.canImport) return
    const bots = step.plan.bots.map((b) => b.key).filter((k) => this.picked('bot', k))
    const groups = step.plan.groups.filter((g) => this.picked('group', g.key) && this.groupAvailable(g.members)).map((g) => g.key)
    await this.run(async () => {
      const result = await (await this.store.ready()).call<ImportResult>('importBots', { path: step.path, bots, groups }, 600_000)
      this.step = { kind: 'done', result }
    })
  }

  private async run(work: () => Promise<void>) {
    if (this.busy) return
    this.busy = true
    this.error = null
    this.changed()
    try {
      await work()
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    } finally {
      this.busy = false
      this.changed()
    }
  }
}
