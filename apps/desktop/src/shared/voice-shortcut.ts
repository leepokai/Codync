// The voice input shortcut (macOS): a double tap of a ⌘ key starts talking to a bot from any app,
// one more tap sends what was said. The key watcher (main/voice-shortcut.ts) reports every
// modifier change; this decides which ones are taps.

/** Settings → General: which ⌘ key, tapped twice, starts voice input. */
export type VoiceShortcut = 'off' | 'rightCommand' | 'leftCommand'

/** A tap of the shortcut's key, or the tap that completes a double tap. */
export type VoiceKey = 'tap' | 'double'

export const DEFAULT_VOICE_SHORTCUT: VoiceShortcut = 'rightCommand'

export const VOICE_SHORTCUTS: { id: VoiceShortcut; label: string }[] = [
  { id: 'off', label: 'Off' },
  { id: 'rightCommand', label: 'Right ⌘ twice' },
  { id: 'leftCommand', label: 'Left ⌘ twice' },
]

/** How the tip names the key ("the right ⌘"), or null when the shortcut is off. */
export const VOICE_SHORTCUT_KEYS: Record<VoiceShortcut, string | null> = { off: null, rightCommand: 'the right ⌘', leftCommand: 'the left ⌘' }

/** One modifier change, as an `NSEvent` reports it. */
export interface ModifierChange {
  /** The virtual key code of the modifier that changed. */
  key: number
  /** `NSEvent.modifierFlags` after the change, with the device bits that tell left from right. */
  flags: number
  /** The event's time, in seconds. */
  at: number
  /** Seconds since the last non-modifier key press or mouse click, in any app. */
  sinceInput: number
}

/** Each ⌘ key's code and its device bit (`NX_DEVICERCMDKEYMASK`, `NX_DEVICELCMDKEYMASK`). */
const COMMANDS = { rightCommand: { key: 54, down: 0x10, other: 0x08 }, leftCommand: { key: 55, down: 0x08, other: 0x10 } }
/** ⇧, ⌃, ⌥ and fn: a chord, not a tap. */
const MODIFIERS = 0x20000 | 0x40000 | 0x80000 | 0x800000
/** The longest press that still counts as a tap. */
const TAP_S = 0.3
/** The longest pause between the first tap's release and the second press. */
const GAP_S = 0.4

/** Taps of one ⌘ key alone: short, nothing else held, no other key pressed or click meanwhile (⌘-click). */
export class CommandTaps {
  private readonly command: (typeof COMMANDS)[keyof typeof COMMANDS]
  private pressed: number | null = null
  private first: { pressed: number; released: number } | null = null

  constructor(shortcut: Exclude<VoiceShortcut, 'off'>) {
    this.command = COMMANDS[shortcut]
  }

  /** Takes the next change: a tap, the second of two quick taps, or nothing. */
  feed(change: ModifierChange): VoiceKey | null {
    const { key, flags, at, sinceInput } = change
    const others = MODIFIERS | this.command.other
    if (key !== this.command.key) return this.reset()
    if (flags & this.command.down) {
      this.pressed = flags & others ? null : at
      return null
    }
    const pressed = this.pressed
    this.pressed = null
    if (pressed === null || flags & others || at - pressed > TAP_S || sinceInput < at - pressed) return this.reset()
    const first = this.first
    // A double tap also needs no key pressed or click between the taps.
    if (first && pressed - first.released <= GAP_S && sinceInput >= at - first.pressed) {
      this.first = null
      return 'double'
    }
    this.first = { pressed, released: at }
    return 'tap'
  }

  private reset() {
    this.pressed = null
    this.first = null
    return null
  }
}

/**
 * What a tap of the shortcut does to dictation: a double tap starts it, a tap after a start by the
 * shortcut sends. Double-tapping to send arrives as that tap then a double, and the double starts nothing.
 */
export class VoiceKeyRouter {
  private sentByTap = false

  route(key: VoiceKey, dictation: { byShortcut: boolean; busy: boolean }): 'start' | 'send' | null {
    const pairsWithSend = this.sentByTap
    this.sentByTap = key === 'tap' && dictation.byShortcut
    if (this.sentByTap) return 'send'
    return key === 'double' && !pairsWithSend && !dictation.busy ? 'start' : null
  }
}
