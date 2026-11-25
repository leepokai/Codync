import { agentNameOf, loginNameOf, modelPath } from '../../shared/model-path.ts'
import type { Bot } from '@shared/models'
import type { BotStore } from '../store/bot-store'

/**
 * The path one bot runs on, assembled once for the whole app: `Codex · gpt-5.3-codex`, or
 * `OpenCode · Work · opendesign/deepseek-v4.1-flash-eco` for a bot that picks a login profile.
 *
 * The roster's hover tip and the Bot Manager's row both call this, so neither keeps its own copy of
 * the three lookups (the MM sync rule: one implementation, no second one to drift). The agent name
 * is the one that computer's hello gives it, and its id when hello doesn't name it — never the
 * `Custom agent` label, which an unknown id would otherwise pick up and read as a second name for
 * the same bot. The login name comes from the same hello's profiles: the computer's own login has
 * no name to add, and an unnamed profile's id is a uuid, which would read as noise.
 *
 * Null when there is nothing to say: a custom command has no model of its own, and a bot with no
 * model set has no path.
 */
export function botModelPath(store: BotStore, bot: Bot): string | null {
  const backends = store.hello?.backends ?? []
  return modelPath(bot, agentNameOf(backends, bot.backend), loginNameOf(backends, bot.backend, bot.loginProfile))
}
