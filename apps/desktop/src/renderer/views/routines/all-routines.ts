// The account menu's Routines: every bot's routines in one list, a section per bot.
import type { Bot } from '../../../shared/models'
import type { Routine } from './routine'

export interface RoutineSection {
  bot: Bot
  routines: Routine[]
}

/** Sections in roster order (`bots`), each bot's routines by name; routines of bots this window doesn't know stay out. */
export function routineSections(routines: Routine[], bots: Bot[]): RoutineSection[] {
  return bots
    .map((bot) => ({ bot, routines: routines.filter((r) => r.botId === bot.id).sort((a, b) => a.name.localeCompare(b.name)) }))
    .filter((s) => s.routines.length > 0)
}

/** The bots a new routine can belong to: agents, not groups. */
export function routineOwners(bots: Bot[]): Bot[] {
  return bots.filter((b) => b.kind !== 'group')
}
