import type { HostClient } from '../client/host-client'
import { Observable } from '../lib/observable.ts'
import type { Routine } from '../views/routines/routine'

/** The account menu's Routines: every bot's routines on one computer, and their switches. */
export class AllRoutinesStore extends Observable {
  routines: Routine[] = []
  loaded = false
  /** A switch is being turned; the others wait for it. */
  busy = false
  error: string | null = null
  private request = 0
  private readonly client: () => Promise<HostClient>

  constructor(client: () => Promise<HostClient>) {
    super()
    this.client = client
  }

  async load() {
    const request = ++this.request
    try {
      const result = await (await this.client()).call<{ routines: Routine[] }>('allRoutines', {})
      if (request === this.request) {
        this.routines = result.routines
        this.loaded = true
        this.error = null
      }
    } catch (error) {
      if (request === this.request) this.error = message(error)
    }
    this.changed()
  }

  async setEnabled(routine: Routine, enabled: boolean) {
    if (this.busy) return
    this.busy = true
    this.changed()
    try {
      await (await this.client()).call('setRoutineEnabled', { botId: routine.botId, id: routine.id, enabled })
      await this.load()
    } catch (error) {
      this.error = message(error)
    } finally {
      this.busy = false
      this.changed()
    }
  }

  /** A routine just saved, shown before the next load brings it. */
  saved(routine: Routine) {
    this.routines = [...this.routines.filter((r) => r.id !== routine.id), routine]
    this.changed()
  }
}

function message(error: unknown) { return error instanceof Error ? error.message : String(error) }
