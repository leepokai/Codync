// The viewer's mouse and keyboard turned into the input messages of the phones' protocol, in
// display points (the Android `ScreenControl`, Direct mode only: the point under the pointer is
// the point on the remote screen). No DOM here: the view passes plain event data.

import { keyCommand, modifierNames, type KeyInfo } from './screen-keys.ts'
import type { Point, ScreenViewport } from './screen-viewport.ts'

export type MouseButton = 'left' | 'middle' | 'right'

export type InputMessage =
  | { type: 'move'; x: number; y: number }
  | { type: 'down' | 'up'; x: number; y: number; button: MouseButton; count: number; modifiers: string[] }
  | { type: 'scroll'; x: number; y: number; dx: number; dy: number; units: 'pixel' }
  | { type: 'key'; key: string; modifiers: string[] }
  | { type: 'text'; text: string }

/** Pointer data in view points (relative to the viewer box). */
export interface PointerInfo extends Point {
  /** DOM `MouseEvent.button`: 0 left, 1 middle, 2 right. */
  button: number
  /** DOM `MouseEvent.detail`: 1, 2, 3 for successive quick clicks. */
  detail: number
  metaKey: boolean
  altKey: boolean
  ctrlKey: boolean
  shiftKey: boolean
}

export interface WheelInfo extends Point {
  deltaX: number
  deltaY: number
  /** DOM `deltaMode`: 0 pixels, 1 lines, 2 pages. */
  deltaMode: number
}

const buttons: Record<number, MouseButton> = { 0: 'left', 1: 'middle', 2: 'right' }
/** What a wheel line and page are worth in pixels. */
const LINE_PIXELS = 16
/** One message carries at most this much text; the data channel's message size is bounded. */
export const MAX_TEXT_CHUNK = 4000

/** Splits pasted text into messages that never cut a character (or a surrogate pair) in two. */
export function textChunks(text: string, size = MAX_TEXT_CHUNK): string[] {
  const chunks: string[] = []
  let current = ''
  for (const character of text) {
    if (current.length + character.length > size) {
      chunks.push(current)
      current = ''
    }
    current += character
  }
  if (current) chunks.push(current)
  return chunks
}

export class ScreenInput {
  viewport: ScreenViewport
  /** The one button held down on the computer (the Mac helper tracks a single one), where it went down and last was. */
  private held: { button: MouseButton; count: number; at: Point } | null = null
  private lastMove: Point | null = null
  private active = true

  private send: (message: InputMessage) => void

  constructor(send: (message: InputMessage) => void, viewport: ScreenViewport) {
    this.send = send
    this.viewport = viewport
  }

  /** View only: nothing reaches the computer, and whatever was held is lifted. */
  get interactive(): boolean {
    return this.active
  }

  set interactive(on: boolean) {
    if (!on) this.releaseAll()
    this.active = on
  }

  get pressed(): boolean {
    return this.held !== null
  }

  /** Returns whether the press was taken (sent, or swallowed because a button is already down). */
  pointerDown(e: PointerInfo): boolean {
    const button = buttons[e.button]
    if (!this.interactive || !button) return false
    if (this.held) return true
    const at = this.viewport.toDisplay(e)
    if (!at) return false
    this.held = { button, count: Math.min(Math.max(e.detail, 1), 3), at }
    this.lastMove = at
    this.send({ type: 'down', ...at, button, count: this.held.count, modifiers: modifierNames(e) })
    return true
  }

  /** A held button follows the pointer even past the video's edge (the end clamps to the display); otherwise only over the video. */
  pointerMove(e: Point) {
    if (!this.interactive) return
    const at = this.held ? this.viewport.clampToDisplay(e) : this.viewport.toDisplay(e)
    if (!at) return
    if (this.lastMove && this.lastMove.x === at.x && this.lastMove.y === at.y) return
    this.lastMove = at
    if (this.held) this.held.at = at
    this.send({ type: 'move', ...at })
  }

  pointerUp(e: PointerInfo) {
    const held = this.held
    if (!held || buttons[e.button] !== held.button) return
    const at = this.viewport.clampToDisplay(e)
    this.release(at, modifierNames(e))
  }

  /** Scrolls the computer at the pointer. Pixels, positive `dy` scrolls the content down, as a browser's wheel does. */
  wheel(e: WheelInfo) {
    if (!this.interactive) return
    const at = this.viewport.toDisplay(e)
    if (!at) return
    const unit = e.deltaMode === 1 ? LINE_PIXELS : e.deltaMode === 2 ? this.viewport.view.height : 1
    const dx = e.deltaX * unit
    const dy = e.deltaY * unit
    if (dx === 0 && dy === 0) return
    this.send({ type: 'scroll', ...at, dx, dy, units: 'pixel' })
  }

  /** Returns whether the key was taken, so the view stops the browser from acting on it too. `paste` gives the clipboard's text. */
  key(e: KeyInfo, platform: string, paste: () => void): boolean {
    if (!this.interactive) return false
    const command = keyCommand(e, platform)
    if (!command) return false
    if (command.type === 'paste') paste()
    else this.send(command)
    return true
  }

  /** Text committed by an IME or pasted. */
  text(text: string) {
    if (!this.interactive) return
    for (const chunk of textChunks(text)) this.send({ type: 'text', text: chunk })
  }

  /** The window lost focus, the connection is being replaced or the viewer closes: lift whatever is held so nothing stays pressed on the computer. */
  releaseAll() {
    if (this.held) this.release(this.held.at, [])
  }

  private release(at: Point, modifiers: string[]) {
    const held = this.held
    if (!held) return
    this.held = null
    this.lastMove = at
    this.send({ type: 'up', ...at, button: held.button, count: held.count, modifiers })
  }
}

/**
 * Ends a viewer's session: whatever the input holds is lifted first, through the session that is still
 * there (the helper keeps a held button after its session closes), then the session is let go and closed.
 * Returns false when there is no session: nothing to lift or close.
 */
export function closeViewerSession(viewer: { session: { close(): void } | null }, input: Pick<ScreenInput, 'releaseAll'>): boolean {
  const session = viewer.session
  if (!session) return false
  input.releaseAll()
  viewer.session = null
  session.close()
  return true
}
