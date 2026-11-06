import type { Bot } from '@shared/models'
import type { HostClient } from '../client/host-client'
import { chainedMoves, placedBots, restoredBots, sectionMembers, sectionName, sectionRename, type SectionMove } from '../lib/roster-sections.ts'

/** A host call that, when refused, runs `undo` before the error shows; resolves to whether it went through. */
type Perform = (op: (c: HostClient) => Promise<unknown>, undo: () => void) => Promise<boolean>

/** One computer's roster sections: moves and renames shown at once, then sent; a refusal takes them back. */
export class BotSections {
  private bots: () => Map<string, Bot>
  private changed: () => void
  private perform: Perform
  /** Moves shown but not answered yet, oldest first. */
  private pending: SectionMove[] = []
  constructor(bots: () => Map<string, Bot>, changed: () => void, perform: Perform) {
    this.bots = bots
    this.changed = changed
    this.perform = perform
  }

  /** Puts the bot in roster section `name`, or takes it out of its section (null). Sends only the section, so a setting changed on another device meanwhile stays. Resolves to whether the host did it. */
  move(bot: Bot, name: string | null): Promise<boolean> {
    const section = sectionName(name)
    return this.moveBots([bot.id], section ?? undefined, (c) => c.call('updateBot', { id: bot.id, section }))
  }

  /**
   * Renames a section on this computer (onto another one's name merges them); null removes it, its bots
   * stay. A refusal runs `onRefused` as its bots go back, in the same update. Resolves to whether the host did it.
   */
  rename(from: string, to: string | null, onRefused?: () => void): Promise<boolean> {
    const rename = sectionRename(from, to)
    return this.moveBots(sectionMembers(this.bots().values(), from), rename.to ?? undefined, (c) => c.call('renameSection', rename), onRefused)
  }

  /**
   * Shows bots `ids` in section `to` (none: undefined) before the host's bot events confirm it, then
   * sends `op`; a refusal takes back the moves nothing changed since. Resolves to whether the host accepted.
   */
  private async moveBots(ids: string[], to: string | undefined, op: (c: HostClient) => Promise<unknown>, onRefused?: () => void): Promise<boolean> {
    const moves = ids.map((id) => ({ id, from: this.bots().get(id)?.section, to }))
    this.pending.push(...moves)
    this.place(placedBots(this.bots(), ids, to))
    try {
      return await this.perform(op, () => {
        onRefused?.()
        // A later move that started where a refused one put the bot now starts where the host still has it.
        for (const move of moves) for (const later of chainedMoves(this.pending, move)) later.from = move.from
        this.place(restoredBots(this.bots(), moves))
      })
    } finally {
      this.pending = this.pending.filter((move) => !moves.includes(move))
    }
  }

  private place(placed: [string, Bot][]) {
    const bots = this.bots()
    for (const [id, bot] of placed) bots.set(id, bot)
    this.changed()
  }
}
