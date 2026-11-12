// What the mouse and keyboard over the remote screen send to the computer: the input events of
// `host/src/screen/mod.rs` (display points; keys named like a Mac keyboard).

export type ButtonName = 'left' | 'middle' | 'right'

export interface KeyInput {
  type: 'key'
  key: string
  modifiers: string[]
}

export type ScreenInput =
  | { type: 'move'; x: number; y: number }
  | { type: 'down' | 'up'; x: number; y: number; button: ButtonName; count: number; modifiers: string[] }
  | { type: 'scroll'; x: number; y: number; dx: number; dy: number; units: 'pixel' }
  | KeyInput
  | { type: 'text'; text: string }
  /** The display shown, and the size it's shown at (device pixels). */
  | { type: 'view'; display: number; long?: number; short?: number }
  | { type: 'clipboard'; text: string }

/** The keyboard state an event carries (a `KeyboardEvent` or a `MouseEvent`). */
export interface Keys {
  metaKey: boolean
  altKey: boolean
  ctrlKey: boolean
  shiftKey: boolean
}

export interface KeyPress extends Keys {
  key: string
  code: string
  isComposing?: boolean
  getModifierState?(key: string): boolean
}

const NAMED: Record<string, string> = {
  Enter: 'return',
  Tab: 'tab',
  Escape: 'escape',
  Backspace: 'delete',
  Delete: 'forwardDelete',
  ArrowLeft: 'left',
  ArrowUp: 'up',
  ArrowRight: 'right',
  ArrowDown: 'down',
  Home: 'home',
  End: 'end',
  PageUp: 'pageUp',
  PageDown: 'pageDown',
}

/**
 * The computer's modifier names. A Mac keyboard on another system uses ⌘ as Ctrl, so ⌘C copies there too;
 * on a Mac the keys stay as they are.
 */
export function modifiers(keys: Keys, local: string, remote: string): string[] {
  const commandIsCtrl = local === 'darwin' && remote !== 'macos'
  const out = new Set<string>()
  if (keys.metaKey) out.add(commandIsCtrl ? 'ctrl' : 'cmd')
  if (keys.ctrlKey) out.add('ctrl')
  if (keys.altKey) out.add('option')
  if (keys.shiftKey) out.add('shift')
  return [...out]
}

/** The key under a shortcut, ignoring what Shift or Option turned it into (`KeyZ` → `z`). */
function baseKey(code: string): string | null {
  const letter = /^Key([A-Z])$/.exec(code)
  if (letter) return letter[1]!.toLowerCase()
  const digit = /^Digit(\d)$/.exec(code)
  return digit ? digit[1]! : null
}

/** A character typed with Option on a Mac, or with AltGr (which Windows reports as Ctrl+Alt): `@` is ⌥L on a German Mac. */
const altTyped = (e: KeyPress, local: string) =>
  e.key.length === 1 && ((local === 'darwin' && e.altKey && !e.metaKey && !e.ctrlKey) || e.getModifierState?.('AltGraph') === true)

/**
 * A key press as a `key` event, or null when typing should carry it instead (plain characters, characters typed with
 * Option or AltGr, an input method composing): those arrive as text from the input field, which knows what was typed.
 */
export function keyInput(e: KeyPress, local: string, remote: string): KeyInput | null {
  if (e.isComposing || e.key === 'Process' || e.key === 'Dead') return null
  const mods = modifiers(e, local, remote)
  const named = NAMED[e.key] ?? (/^F([1-9]|1[0-2])$/.test(e.key) ? e.key.toLowerCase() : null)
  if (named) return { type: 'key', key: named, modifiers: mods }
  const shortcut = e.metaKey || e.ctrlKey || e.altKey
  if (!shortcut || altTyped(e, local)) return null
  if (e.key === ' ') return { type: 'key', key: 'space', modifiers: mods }
  const key = baseKey(e.code) ?? (e.key.length === 1 ? e.key.toLowerCase() : null)
  return key ? { type: 'key', key, modifiers: mods } : null
}

/** Whether a press pastes here: the local clipboard goes over first, then the shortcut. */
export const isPaste = (e: KeyPress, local: string) => baseKey(e.code) === 'v' && (local === 'darwin' ? e.metaKey : e.ctrlKey) && !e.altKey

export interface Box {
  left: number
  top: number
  width: number
  height: number
}

export interface Size {
  width: number
  height: number
}

/** Where the picture sits in its box when it's scaled to fit (letterboxed, like `object-fit: contain`). */
export function fitted(box: Box, picture: Size): Box {
  if (picture.width <= 0 || picture.height <= 0) return box
  const scale = Math.min(box.width / picture.width, box.height / picture.height)
  const width = picture.width * scale
  const height = picture.height * scale
  return { left: box.left + (box.width - width) / 2, top: box.top + (box.height - height) / 2, width, height }
}

/** A pointer position over the video as a point on the display, or null outside the picture. */
export function displayPoint(x: number, y: number, box: Box, picture: Size, display: Size): { x: number; y: number } | null {
  const shown = fitted(box, picture)
  if (shown.width <= 0 || shown.height <= 0) return null
  const u = (x - shown.left) / shown.width
  const v = (y - shown.top) / shown.height
  if (u < 0 || u > 1 || v < 0 || v > 1) return null
  return { x: u * display.width, y: v * display.height }
}

/** `MouseEvent.button` as the protocol's button name. */
export const buttonName = (button: number): ButtonName => (button === 2 ? 'right' : button === 1 ? 'middle' : 'left')

/** A wheel's travel in pixels (lines and pages are converted the way browsers scroll them). */
export function wheelPixels(e: { deltaX: number; deltaY: number; deltaMode: number }, page: number): { dx: number; dy: number } {
  const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? page : 1
  return { dx: e.deltaX * unit, dy: e.deltaY * unit }
}
