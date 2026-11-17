import { useEffect, useRef, type RefObject } from 'react'
import { buttonName, displayPoint, isPaste, keyInput, modifiers, wheelPixels, type ButtonName, type KeyInput, type Keys, type ScreenInput } from '../../lib/screen-input'
import type { ScreenSession } from '../../store/screen-session'

interface Targets {
  stage: RefObject<HTMLDivElement | null>
  video: RefObject<HTMLVideoElement | null>
  /** A hidden text field that holds the keyboard, so typing and input methods arrive as text. */
  keys: RefObject<HTMLTextAreaElement | null>
}

interface State {
  session: ScreenSession | null
  /** The user has the controls (not just watching). */
  enabled: boolean
  /** The computer's platform (`macos`, `linux`, …), for how ⌘ maps there. */
  remote: string
}

/** Input goes to the computer only while you have the controls of a live screen. */
const controlling = (s: State) => s.enabled && s.session?.phase.kind === 'live'

/**
 * Sends the mouse and keyboard over the picture to the computer while `enabled`: real button presses and
 * releases (with the click count, so double-clicks stay double-clicks), moves and wheel travel once per
 * frame, keys and shortcuts, typed text, and the clipboard with a paste.
 * The hidden field carries `data-captures-keys` while you have control, so the sheet leaves Escape and Tab to it.
 */
export function useScreenInput({ stage, video, keys }: Targets, state: State) {
  const current = useRef(state)
  current.current = state
  useEffect(() => {
    const area = stage.current
    const field = keys.current
    if (!area || !field) return
    const local = window.codync.platform
    const live = () => (controlling(current.current) ? current.current.session : null)
    const mods = (e: Keys) => modifiers(e, local, current.current.remote)
    let last: { x: number; y: number } | null = null
    const point = (x: number, y: number) => {
      const session = live()
      const v = video.current
      if (!session || !v) return null
      const p = displayPoint(x, y, v.getBoundingClientRect(), { width: v.videoWidth, height: v.videoHeight }, session.display)
      if (p) last = p
      return p
    }
    const send = (event: ScreenInput) => live()?.send(event)

    let move: { x: number; y: number } | null = null
    let moveFrame = 0
    const flushMove = () => {
      if (moveFrame) cancelAnimationFrame(moveFrame)
      moveFrame = 0
      if (move) send({ type: 'move', ...move })
      move = null
    }
    let held: ButtonName | null = null
    const onDown = (e: MouseEvent) => {
      // Back and forward buttons have no counterpart on the computer.
      if (e.button > 2) return
      const p = point(e.clientX, e.clientY)
      if (!p) return
      e.preventDefault()
      field.focus({ preventScroll: true })
      flushMove()
      held = buttonName(e.button)
      send({ type: 'down', ...p, button: held, count: e.detail || 1, modifiers: mods(e) })
    }
    const onMove = (e: MouseEvent) => {
      const p = point(e.clientX, e.clientY)
      if (!p) return
      move = p
      if (!moveFrame) moveFrame = requestAnimationFrame(flushMove)
    }
    const onUp = (e: MouseEvent) => {
      if (!held) return
      const button = held
      held = null
      const p = point(e.clientX, e.clientY) ?? last
      flushMove()
      if (p) send({ type: 'up', ...p, button, count: e.detail || 1, modifiers: mods(e) })
    }
    const onMenu = (e: MouseEvent) => {
      if (live()) e.preventDefault()
    }

    let wheel = { dx: 0, dy: 0 }
    let wheelAt: { x: number; y: number } | null = null
    let wheelFrame = 0
    const onWheel = (e: WheelEvent) => {
      const p = point(e.clientX, e.clientY)
      if (!p) return
      e.preventDefault()
      const d = wheelPixels(e, area.clientHeight)
      wheel = { dx: wheel.dx + d.dx, dy: wheel.dy + d.dy }
      wheelAt = p
      if (wheelFrame) return
      wheelFrame = requestAnimationFrame(() => {
        wheelFrame = 0
        if (wheelAt) send({ type: 'scroll', ...wheelAt, ...wheel, units: 'pixel' })
        wheel = { dx: 0, dy: 0 }
        wheelAt = null
      })
    }

    let paste: KeyInput | null = null
    const onKey = (e: KeyboardEvent) => {
      if (document.activeElement !== field || !live()) return
      // Paste carries the clipboard over first: the press waits for the paste event that brings the text.
      if (isPaste(e, local)) {
        paste = keyInput(e, local, current.current.remote)
        return
      }
      const input = keyInput(e, local, current.current.remote)
      if (!input) return
      e.preventDefault()
      send(input)
    }
    const onInput = (e: Event) => {
      const typed = e as InputEvent
      if (typed.isComposing) return
      if (typed.inputType === 'insertText' && typed.data) send({ type: 'text', text: typed.data })
      field.value = ''
    }
    const onComposed = (e: CompositionEvent) => {
      if (e.data) send({ type: 'text', text: e.data })
      field.value = ''
    }
    const onPaste = (e: ClipboardEvent) => {
      e.preventDefault()
      const session = live()
      const text = e.clipboardData?.getData('text/plain')
      if (session && text) session.pushClipboard(text)
      if (paste) send(paste)
      paste = null
    }

    area.addEventListener('mousedown', onDown)
    area.addEventListener('contextmenu', onMenu)
    area.addEventListener('wheel', onWheel, { passive: false })
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
    window.addEventListener('keydown', onKey, true)
    field.addEventListener('input', onInput)
    field.addEventListener('compositionend', onComposed)
    field.addEventListener('paste', onPaste)
    return () => {
      cancelAnimationFrame(moveFrame)
      cancelAnimationFrame(wheelFrame)
      area.removeEventListener('mousedown', onDown)
      area.removeEventListener('contextmenu', onMenu)
      area.removeEventListener('wheel', onWheel)
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
      window.removeEventListener('keydown', onKey, true)
      field.removeEventListener('input', onInput)
      field.removeEventListener('compositionend', onComposed)
      field.removeEventListener('paste', onPaste)
    }
  }, [stage, video, keys])

  // While you control a live screen and its field has focus, menu shortcuts (⌘F, ⌘R, …) go to the computer
  // instead of acting here; a dropped connection gives them back.
  const capturing = controlling(state)
  useEffect(() => {
    const field = keys.current
    if (!field || !capturing) return
    const report = () => window.codync.app.remoteKeyboard(document.activeElement === field)
    report()
    field.addEventListener('focus', report)
    field.addEventListener('blur', report)
    return () => {
      field.removeEventListener('focus', report)
      field.removeEventListener('blur', report)
      window.codync.app.remoteKeyboard(false)
    }
  }, [keys, capturing])
}
