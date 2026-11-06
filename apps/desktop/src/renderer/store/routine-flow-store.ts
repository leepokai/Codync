import type { HostClient } from '../client/host-client'
import { Observable } from '../lib/observable.ts'
import { clampCoordinate, type FlowCanvas, type FlowLink } from '../lib/routine-flow.ts'

/** What a view needs after a move: the node's new place, shown at once and saved on the host. */
type Client = () => Promise<Pick<HostClient, 'call'>>

/**
 * One bot's routine-flow canvas. The host owns the flow (rules, runs, steps); this keeps what is
 * loaded, the request in flight and the last error. A change reloads the canvas, and so does a
 * `routineFlow` event (the store's `flowRev` for the bot moving).
 */
export class RoutineFlowStore extends Observable {
  canvas: FlowCanvas | null = null
  loading = false
  busy = false
  error: string | null = null
  /** Set when the host has no flow methods (an older host): the canvas hides. */
  unsupported = false
  private sequence = 0
  private disposed = false

  readonly botId: string
  private readonly client: Client

  constructor(botId: string, client: Client) {
    super()
    this.botId = botId
    this.client = client
  }

  /** A view mounted again (React may mount twice): the store works again. */
  resume() {
    this.disposed = false
  }

  dispose() {
    this.disposed = true
  }

  private fail(error: unknown) {
    const text = error instanceof Error ? error.message : String(error)
    if (/unknown method|404/i.test(text)) this.unsupported = true
    this.error = text
  }

  async load() {
    if (this.disposed) return
    const sequence = ++this.sequence
    this.loading = true
    this.changed()
    try {
      const canvas = await (await this.client()).call<FlowCanvas>('routineFlowCanvas', { botId: this.botId })
      if (sequence !== this.sequence || this.disposed) return
      this.canvas = canvas
      this.error = null
      this.unsupported = false
    } catch (error) {
      if (sequence === this.sequence && !this.disposed) this.fail(error)
    } finally {
      if (sequence === this.sequence && !this.disposed) {
        this.loading = false
        this.changed()
      }
    }
  }

  /** A change the user made: runs it, then loads the canvas again. Whether it worked. */
  private async change<T>(method: string, body: object): Promise<T | null> {
    if (this.disposed) return null
    this.busy = true
    this.error = null
    this.changed()
    try {
      const answer = await (await this.client()).call<T>(method, body)
      await this.load()
      return answer
    } catch (error) {
      this.fail(error)
      return null
    } finally {
      this.busy = false
      this.changed()
    }
  }

  /** A node moved: shown where it was dropped at once, then saved. */
  async move(nodeKey: string, x: number, y: number) {
    const place = { nodeKey, x: clampCoordinate(x), y: clampCoordinate(y) }
    if (this.canvas) {
      this.canvas = { ...this.canvas, positions: [...this.canvas.positions.filter((p) => p.nodeKey !== nodeKey), place] }
      this.changed()
    }
    await this.change('saveRoutineFlowPosition', { botId: this.botId, ...place })
  }

  async place(placedBotId: string, x: number, y: number) {
    await this.change('placeRoutineFlowBot', { botId: this.botId, placedBotId, x: clampCoordinate(x), y: clampCoordinate(y) })
  }

  async removeNode(nodeKey: string) {
    await this.change('removeRoutineFlowPosition', { botId: this.botId, nodeKey })
  }

  async connect(routineId: string, fromBotId: string, toBotId: string, instruction: string): Promise<FlowLink | null> {
    const answer = await this.change<{ link: FlowLink }>('connectRoutineFlow', { routineId, fromBotId, toBotId, instruction })
    return answer?.link ?? null
  }

  async updateLink(linkId: string, instruction: string) {
    await this.change('updateRoutineFlowLink', { linkId, instruction })
  }

  async disconnect(linkId: string) {
    await this.change('disconnectRoutineFlow', { linkId })
  }

  /** Starts the flow with a sample reply, without running the routine. */
  async test(routineId: string) {
    await this.change('testRoutineFlow', { botId: this.botId, routineId })
  }

  /** Pauses or resumes a routine from the inspector. */
  async setEnabled(routineId: string, enabled: boolean) {
    await this.change('setRoutineEnabled', { botId: this.botId, id: routineId, enabled })
  }
}
