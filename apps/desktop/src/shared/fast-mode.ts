// Fast mode in the chat (the composer's `/fast`) and bot settings; the host reports what the agent
// offers (`fastModeSupport`, `fastModeIssue`): docs/features/marketplace.md, "Fast mode".
import type { Bot } from './models'

export type FastModeState = 'on' | 'off' | 'refused'

/**
 * An agent bot unless its agent said it has no Fast mode (an unknown one may: its first session
 * settles it), or one that has it on (so it can be turned off).
 */
export function fastModeShown(bot: Bot | null): boolean {
  if (!bot || bot.kind === 'group') return false
  return bot.fastMode || bot.fastModeSupport !== 'unsupported'
}

export function fastModeState(bot: Bot): FastModeState {
  if (!bot.fastMode) return 'off'
  return bot.fastModeSupport === 'refused' ? 'refused' : 'on'
}

/** Why it isn't in effect, when the agent turned it off itself. */
export function fastModeRefusal(bot: Bot): string | null {
  if (fastModeState(bot) !== 'refused') return null
  return `The agent turned it off: ${bot.fastModeIssue ?? 'not available'}. Switch it off and on to try again.`
}

/**
 * A group's `/fast`: its members that show Fast mode, on once all of them have it on. Picking it
 * sets each one's own Fast mode, so it follows them into their own chats.
 */
export function groupFastMode(members: Bot[]): { members: Bot[]; state: FastModeState; detail: string } | null {
  const shown = members.filter(fastModeShown)
  if (!shown.length) return null
  if (!shown.every((m) => m.fastMode)) return { members: shown, state: 'off', detail: 'Turn on for every bot here: faster replies and more usage' }
  const refused = shown.find((m) => fastModeState(m) === 'refused')
  if (refused) return { members: shown, state: 'refused', detail: `${refused.name}: ${fastModeRefusal(refused)}` }
  return { members: shown, state: 'on', detail: 'Turn off for every bot here and return to standard speed' }
}

/** What picking `/fast` in the composer's command list does, or why it isn't in effect. */
export function fastModeDetail(bot: Bot): string {
  switch (fastModeState(bot)) {
    case 'off':
      return 'Turn on for faster replies and more usage'
    case 'on':
      return 'Turn off and return to standard speed'
    case 'refused':
      return fastModeRefusal(bot) ?? ''
  }
}
