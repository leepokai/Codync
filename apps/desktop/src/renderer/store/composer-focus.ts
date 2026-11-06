import type { BotRef } from './app-model'

/** The chat whose message box takes the caret when it shows: one opened by a click, not reached with ↑/↓ through the roster. */
export class ComposerFocus {
  target: BotRef | null = null

  open(ref: BotRef) {
    this.target = { ...ref }
  }

  /** The selection moved: a click elsewhere no longer brings the caret anywhere. */
  select(ref: BotRef | null) {
    if (!same(ref, this.target)) this.target = null
  }

  /** Whether the chat `ref` was just opened by a click; answers true once. */
  take(ref: BotRef) {
    if (!same(this.target, ref)) return false
    this.target = null
    return true
  }
}

const same = (a: BotRef | null, b: BotRef | null) => !!a && !!b && a.computerId === b.computerId && a.botId === b.botId
