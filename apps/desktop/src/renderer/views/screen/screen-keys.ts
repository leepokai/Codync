// Hardware keys as the computer names them (the Android `ScreenKeys`, with the desktop's keys).
// The protocol has no key-up: `key` presses a key with its modifiers, `text` types characters.

/** The parts of a DOM `KeyboardEvent` the mapping reads. */
export interface KeyInfo {
  key: string
  code: string
  metaKey: boolean
  altKey: boolean
  ctrlKey: boolean
  shiftKey: boolean
  /** The AltGr key is down (Windows and Linux layouts type `@` or `€` with it; it reports Ctrl+Alt). */
  altGraph: boolean
  /** An IME is composing: its commit arrives as text instead. */
  isComposing: boolean
}

export type KeyCommand =
  | { type: 'key'; key: string; modifiers: string[] }
  | { type: 'text'; text: string }
  /** Cmd+V (Ctrl+V off the Mac): the viewer types what the local clipboard holds. */
  | { type: 'paste' }

const named: Record<string, string> = {
  Enter: 'return', Backspace: 'delete', Delete: 'forwardDelete', Escape: 'escape', Tab: 'tab',
  ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right',
  Home: 'home', End: 'end', PageUp: 'pageUp', PageDown: 'pageDown',
  ...Object.fromEntries(Array.from({ length: 12 }, (_, i) => [`F${i + 1}`, `f${i + 1}`])),
}

/** Keys that only hold a modifier or a lock: they send nothing on their own. */
const silent = new Set(['Shift', 'Control', 'Alt', 'AltGraph', 'Meta', 'OS', 'CapsLock', 'NumLock', 'ScrollLock', 'Fn', 'FnLock', 'Hyper', 'Super', 'Symbol', 'SymbolLock', 'Dead', 'Process', 'Unidentified'])

/** Cmd always reads `cmd`, as on the phones; the computer decides what that means to it. Order: cmd, option, ctrl, shift. */
export function modifierNames(e: Pick<KeyInfo, 'metaKey' | 'altKey' | 'ctrlKey' | 'shiftKey'>): string[] {
  return [e.metaKey && 'cmd', e.altKey && 'option', e.ctrlKey && 'ctrl', e.shiftKey && 'shift'].filter((m): m is string => !!m)
}

/**
 * The key's name without modifiers: the layout's Latin letter (AZERTY's Cmd+A is `a`, though it sits on `KeyQ`),
 * else the physical letter (a Cyrillic layout's Cmd+С is still C), the digit, or the character.
 */
function baseCharacter(e: KeyInfo): string | null {
  if (/^[A-Za-z]$/.test(e.key)) return e.key.toLowerCase()
  const letter = /^Key([A-Z])$/.exec(e.code)
  if (letter) return letter[1].toLowerCase()
  const digit = /^Digit([0-9])$/.exec(e.code)
  if (digit) return digit[1]
  if (e.code === 'Space' || e.key === ' ') return 'space'
  return [...e.key].length === 1 ? e.key.toLowerCase() : null
}

/** What a key press sends, or null for nothing (a lone modifier, a dead key, an unknown key, a composing IME). */
export function keyCommand(e: KeyInfo, platform: string): KeyCommand | null {
  if (e.isComposing || silent.has(e.key)) return null
  const modifiers = modifierNames(e)
  const pasteModifier = platform === 'darwin' ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey
  if (pasteModifier && !e.altKey && !e.shiftKey && e.code === 'KeyV') return { type: 'paste' }
  const special = named[e.key]
  if (special) return { type: 'key', key: special, modifiers }
  const printable = [...e.key].length === 1
  if (e.altGraph) return printable ? { type: 'text', text: e.key } : null
  const chord = e.metaKey || e.ctrlKey || e.altKey
  if (chord) {
    const key = baseCharacter(e)
    return key ? { type: 'key', key, modifiers } : null
  }
  return printable ? { type: 'text', text: e.key } : null
}
