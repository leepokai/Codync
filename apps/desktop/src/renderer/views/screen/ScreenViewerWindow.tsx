import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { DropdownMenu, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useModel } from '../../lib/observable'
import { screenAvailable } from './screen-access'
import { accessGuidance, failureText } from './screen-guidance'
import type { PointerInfo } from './screen-input'
import { ScreenViewerModel } from './screen-viewer-model'
import './screen-viewer.css'

/** The Remote screen window: a computer's display with its mouse and keyboard (the phones' `ScreenView`, for a desktop). */
export function ScreenViewerWindow({ computerId }: { computerId: string }) {
  const tr = useTr()
  const [model] = useState(() => new ScreenViewerModel(computerId))
  useModel(model)
  const session = useModel(model.session)
  const screen = model.screen
  const stage = useRef<HTMLDivElement>(null)
  const keys = useRef<HTMLTextAreaElement>(null)
  const video = useRef<HTMLVideoElement>(null)

  useEffect(() => {
    model.start()
    // Closing the window for any reason ends the session on the computer.
    const stop = () => model.stop()
    window.addEventListener('pagehide', stop)
    return () => {
      window.removeEventListener('pagehide', stop)
      stop()
    }
  }, [model])

  useEffect(() => {
    document.title = `${tr('Remote screen')}${model.name ? ` · ${model.name}` : ''}`
  }, [tr, model.name])

  const stream = session?.stream ?? null
  // The video element exists only while the screen can be shown, which may come after the stream.
  const showing = !!session && !!model.display && screenAvailable(screen)
  useEffect(() => { if (video.current) video.current.srcObject = stream }, [stream, showing])

  useStageSize(stage, model)
  usePointer(stage, keys, model)
  useEffect(() => {
    const focus = () => keys.current?.focus({ preventScroll: true })
    // Losing focus lifts what is held: nothing stays pressed on the computer.
    const blur = () => model.input.releaseAll()
    focus()
    window.addEventListener('focus', focus)
    window.addEventListener('blur', blur)
    return () => {
      window.removeEventListener('focus', focus)
      window.removeEventListener('blur', blur)
    }
  }, [model])

  const phase = session?.phase
  const display = model.display
  const frame = model.input.viewport.frame
  const live = !!session && !!display && screenAvailable(screen)
  const agentWorking = !!screen?.agentBot && !model.interactive
  const displays = screen?.displays ?? []

  const status = (() => {
    if (!live || !phase) return model.unreachable ? tr('Offline') : tr('Connecting…')
    switch (phase.kind) {
      case 'live': return tr('Connected')
      case 'reconnecting': return tr('Reconnecting…')
      case 'failed': return failureText(phase.failure, tr)
      default: return tr('Connecting…')
    }
  })()
  const tone = live && phase?.kind === 'live' ? 'ok' : live && phase?.kind === 'failed' ? 'error' : 'busy'

  const onKeyDown = (e: React.KeyboardEvent) => {
    const taken = model.input.key({
      key: e.key, code: e.code, metaKey: e.metaKey, altKey: e.altKey, ctrlKey: e.ctrlKey, shiftKey: e.shiftKey,
      altGraph: e.getModifierState('AltGraph'), isComposing: e.nativeEvent.isComposing,
    }, window.codync.platform, () => void model.paste())
    if (taken) e.preventDefault()
  }

  return (
    <VStack className="screen-viewer">
      {/* Buttons here don't take the keyboard: it stays with the remote screen. */}
      <HStack gap={8} className="screen-bar" onMouseDown={(e) => e.preventDefault()}>
        <IconButton title="Close" icon="xmark" size={28} onClick={() => { model.stop(); window.codync.screenViewer.close() }} />
        <Text size="headline" truncate>{model.name}</Text>
        <span className={`screen-status ${tone}`}>
          <span className="screen-dot" />
          <Text size="caption" tone="secondary" truncate>{status}</Text>
        </span>
        {phase?.kind === 'failed' ? <IconButton title="Try again" icon="arrow.clockwise" size={28} onClick={() => model.retry()} /> : null}
        {agentWorking ? (
          <HStack gap={6} className="screen-agent">
            <Icon name="cursorarrow.motionlines" size={12} color="var(--accent)" />
            <Text size="caption" truncate>{tr('Using the computer')}</Text>
          </HStack>
        ) : null}
        <Spacer />
        {session && model.interactive ? (
          <IconButton title="Send my clipboard" icon="arrow.up.doc.on.clipboard" size={28} onClick={() => void model.sendClipboard()} />
        ) : null}
        {session?.remoteClipboard != null ? (
          <IconButton title="Copy from computer" icon="arrow.down.doc.on.clipboard" size={28} onClick={() => model.copyFromComputer()} />
        ) : null}
        {session && displays.length > 1 ? (
          <DropdownMenu className="icon-button screen-displays" title={tr('Displays')} items={() => displays.map((d) => ({ title: d.name, verbatim: true, icon: 'display', selected: d.id === display?.id, action: () => model.switchDisplay(d) }))}>
            <Icon name="display.2" size={14} weight="medium" />
          </DropdownMenu>
        ) : null}
        {session ? (
          <IconButton title={model.interactive ? 'Hand back to bots' : 'Take over'} icon={model.interactive ? 'hand.raised.slash' : 'hand.raised'} size={28}
            disabled={screen?.input !== true} onClick={() => model.setInteractive(!model.interactive)} />
        ) : null}
      </HStack>
      <div ref={stage} className="screen-stage" onContextMenu={(e) => e.preventDefault()}>
        <textarea ref={keys} className="screen-keys" aria-label={tr('Remote screen')} autoCapitalize="off" autoCorrect="off" spellCheck={false}
          onKeyDown={onKeyDown}
          onCompositionEnd={(e) => { model.input.text(e.data); e.currentTarget.value = '' }}
          onInput={(e) => { if (!(e.nativeEvent as InputEvent).isComposing) e.currentTarget.value = '' }} />
        {live ? (
          <video ref={video} className="screen-video" autoPlay muted playsInline
            style={{ left: frame.x, top: frame.y, width: frame.width, height: frame.height }} />
        ) : null}
        <Overlay model={model} live={live} />
      </div>
    </VStack>
  )
}

/** What covers or sits over the video: progress, errors, why the screen isn't available, and the view-only note. */
function Overlay({ model, live }: { model: ScreenViewerModel; live: boolean }) {
  const tr = useTr()
  const { screen, session } = model
  const phase = session?.phase
  if (!live) {
    if (model.unreachable && !screen) return <Notice icon="wifi.slash" text={tr('This computer is offline.')} />
    if (!screen) return <Notice text={tr('Connecting to the screen…')} />
    return <Notice icon="lock.display" text={accessGuidance(screen, tr)} />
  }
  if (phase?.kind === 'connecting') return <Notice text={tr('Connecting to the screen…')} />
  if (phase?.kind === 'reconnecting') return <Notice text={tr('Reconnecting…')} dim />
  if (phase?.kind === 'failed') return <Notice icon="exclamationmark.triangle" text={failureText(phase.failure, tr)} retry={() => model.retry()} />
  const note = screen && !screen.input
    ? tr('View only. To allow control, open Codync → Settings → Computer access on the computer.')
    : session?.lastError ?? null
  return note ? <Text className="screen-note" size="caption">{note}</Text> : null
}

/** A centered icon (or a spinner), a line and an optional retry. */
function Notice({ icon, text, retry, dim = false }: { icon?: string; text: string; retry?: () => void; dim?: boolean }) {
  return (
    <VStack gap={12} align="center" className={`screen-notice ${dim ? 'dim' : ''}`}>
      {icon ? <Icon name={icon} size={36} color="var(--secondary)" /> : <Spinner size={22} />}
      <Text size="callout" tone="secondary">{text}</Text>
      {retry ? <IconButton title="Try again" icon="arrow.clockwise" size={32} onClick={retry} /> : null}
    </VStack>
  )
}

/** The video box's size and the device pixel ratio, kept on the model: they place the video and the pointer. */
function useStageSize(stage: React.RefObject<HTMLDivElement | null>, model: ScreenViewerModel) {
  useLayoutEffect(() => {
    const element = stage.current
    if (!element) return
    const measure = () => {
      const { width, height } = element.getBoundingClientRect()
      model.setView({ width, height }, window.devicePixelRatio || 1)
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    // Moving the window to another display changes the ratio without resizing it.
    let query = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`)
    const ratio = () => {
      measure()
      query.removeEventListener('change', ratio)
      query = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`)
      query.addEventListener('change', ratio)
    }
    query.addEventListener('change', ratio)
    window.addEventListener('resize', measure)
    return () => {
      observer.disconnect()
      query.removeEventListener('change', ratio)
      window.removeEventListener('resize', measure)
    }
  }, [stage, model])
}

/** Mouse and wheel: presses start on the stage and follow the pointer anywhere until released. */
function usePointer(stage: React.RefObject<HTMLDivElement | null>, keys: React.RefObject<HTMLTextAreaElement | null>, model: ScreenViewerModel) {
  useEffect(() => {
    const element = stage.current
    if (!element) return
    const at = (e: MouseEvent) => {
      const rect = element.getBoundingClientRect()
      return { x: e.clientX - rect.left, y: e.clientY - rect.top }
    }
    const info = (e: MouseEvent): PointerInfo => ({ ...at(e), button: e.button, detail: e.detail, metaKey: e.metaKey, altKey: e.altKey, ctrlKey: e.ctrlKey, shiftKey: e.shiftKey })
    // One move per frame: the pointer reports faster than the computer can use.
    let pending: MouseEvent | null = null
    let frame = 0
    const flush = () => {
      cancelAnimationFrame(frame)
      frame = 0
      if (pending) model.input.pointerMove(at(pending))
      pending = null
    }
    const down = (e: MouseEvent) => {
      // Whether or not the press is sent (a black bar, view only), the page must not take the keyboard from the remote screen.
      e.preventDefault()
      keys.current?.focus({ preventScroll: true })
      model.input.pointerDown(info(e))
    }
    const move = (e: MouseEvent) => {
      pending = e
      if (!frame) frame = requestAnimationFrame(flush)
    }
    const up = (e: MouseEvent) => {
      flush()
      model.input.pointerUp(info(e))
    }
    const wheel = (e: WheelEvent) => {
      // A page-level listener would be passive: the stage must stop the app from scrolling too.
      e.preventDefault()
      model.input.wheel({ ...at(e), deltaX: e.deltaX, deltaY: e.deltaY, deltaMode: e.deltaMode })
    }
    element.addEventListener('mousedown', down)
    element.addEventListener('wheel', wheel, { passive: false })
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
    return () => {
      cancelAnimationFrame(frame)
      element.removeEventListener('mousedown', down)
      element.removeEventListener('wheel', wheel)
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
    }
  }, [stage, keys, model])
}
