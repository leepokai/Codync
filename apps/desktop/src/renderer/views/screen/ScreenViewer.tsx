import { useEffect, useRef, useState } from 'react'
import { Button, DropdownMenu, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { fill } from '../../lib/fill'
import { useModel } from '../../lib/observable'
import { mainDisplay, screenAvailable, screenGuidance } from '../../lib/screen-state'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import type { BotStore } from '../../store/bot-store'
import { ScreenSession } from '../../store/screen-session'
import { useScreenInput } from './use-screen-input'
import './screen-viewer.css'

/**
 * Another computer's screen in a sheet over the window (the iPhone's `ScreenView`): the picture, and the mouse and
 * keyboard over it once you take over from the bots.
 */
export function ScreenViewer({ store }: { store: BotStore }) {
  const tr = useTr()
  useModel(store)
  useScreenViewed('remote_screen')
  const screen = store.screen
  const online = store.connection.kind === 'online'
  const available = online && screenAvailable(screen)
  const session = useModel(useSession(store, available))
  const [interactive, setInteractive] = useState(false)
  const tookOver = useRef(false)
  const stage = useRef<HTMLDivElement>(null)
  const video = useRef<HTMLVideoElement>(null)
  const keys = useRef<HTMLTextAreaElement>(null)
  const canControl = screen?.input === true
  useScreenInput({ stage, video, keys }, { session, enabled: interactive, remote: screen?.platform ?? '' })

  // Take the controls (bots may still look); hand them back when this closes or control isn't allowed.
  const control = (on: boolean) => {
    const allowed = on && canControl
    setInteractive(allowed)
    if (allowed) keys.current?.focus({ preventScroll: true })
    if (allowed === tookOver.current) return
    tookOver.current = allowed
    store.screenTakeover(allowed)
  }
  useEffect(() => {
    if (session) control(true)
  }, [session])
  useEffect(() => {
    if (!canControl) control(false)
  }, [canControl])
  // Back live after a failure or a long outage, the host may have handed control back to the bots when its last
  // viewer went: ask again.
  const live = session?.phase.kind === 'live'
  useEffect(() => {
    if (live && tookOver.current && store.screen?.userControl === false) store.screenTakeover(true)
  }, [live])
  useEffect(() => () => {
    if (tookOver.current) store.screenTakeover(false)
  }, [store])

  useEffect(() => {
    if (video.current) video.current.srcObject = session?.video ?? null
  }, [session?.video])
  useViewSize(stage, session)

  const agent = screen?.agentBot ? store.bots.get(screen.agentBot) : null
  const subtitle = agent && !interactive ? fill(tr('{name} is using the computer'), { name: agent.name }) : undefined
  const displays = screen?.displays ?? []
  const trailing = session ? (
    <>
      {session.remoteClipboard !== null ? (
        <IconButton title="Copy from computer" icon="doc.on.clipboard.fill" onClick={() => {
          const text = session.takeRemoteClipboard()
          if (text) void navigator.clipboard.writeText(text)
        }} />
      ) : null}
      {displays.length > 1 ? (
        <DropdownMenu className="icon-button screen-viewer-menu" title={tr('Displays')} items={() => displays.map((d) => ({
          title: d.name, icon: 'display', verbatim: true, selected: d.id === session.display.id, action: () => session.switchDisplay(d),
        }))}>
          <Icon name="display.2" size={14} weight="medium" />
        </DropdownMenu>
      ) : null}
      <IconButton title={interactive ? 'Hand back to bots' : 'Take over'} icon={interactive ? 'hand.raised.slash' : 'hand.raised'} selected={interactive} disabled={!canControl} onClick={() => control(!interactive)} />
    </>
  ) : null

  return (
    <SheetPage className="screen-viewer">
      <ModalHeader icon="display" title={store.hostName} subtitle={subtitle} trailing={trailing} />
      <div ref={stage} className="screen-viewer-stage">
        <video ref={video} className="screen-viewer-video" autoPlay muted playsInline hidden={!session?.video} />
        <textarea ref={keys} className="screen-viewer-keys" aria-label={tr('Keyboard for the remote screen')} autoComplete="off" autoCorrect="off" spellCheck={false} tabIndex={-1}
          data-captures-keys={interactive && live ? '' : undefined} />
        <ScreenStatus store={store} session={session} available={available} canControl={canControl} />
      </div>
    </SheetPage>
  )
}

/** One session while the screen can be watched; it closes when this does or the screen goes away. */
function useSession(store: BotStore, available: boolean) {
  const [session, setSession] = useState<ScreenSession | null>(null)
  useEffect(() => {
    if (!available) return
    let live: ScreenSession | null = null
    let cancelled = false
    void store.ready().then((client) => {
      const display = store.screen && mainDisplay(store.screen)
      if (cancelled || !display) return
      live = new ScreenSession(client, store.hostRoute === 'relay', display)
      setSession(live)
      void live.start()
    }, () => {})
    return () => {
      cancelled = true
      live?.close()
      setSession(null)
    }
  }, [store, available])
  return session
}

/** Tells the computer how big the picture is shown, so it sends no more pixels than that. */
function useViewSize(stage: React.RefObject<HTMLDivElement | null>, session: ScreenSession | null) {
  const live = session?.phase.kind === 'live'
  useEffect(() => {
    const area = stage.current
    if (!area || !session || !live) return
    let timer: ReturnType<typeof setTimeout> | undefined
    const report = () => {
      const scale = window.devicePixelRatio || 1
      const w = area.clientWidth * scale
      const h = area.clientHeight * scale
      session.view(Math.max(w, h), Math.min(w, h))
    }
    const observer = new ResizeObserver(() => {
      clearTimeout(timer)
      timer = setTimeout(report, 200)
    })
    observer.observe(area)
    report()
    return () => {
      clearTimeout(timer)
      observer.disconnect()
    }
  }, [stage, session, live])
}

function ScreenStatus({ store, session, available, canControl }: { store: BotStore; session: ScreenSession | null; available: boolean; canControl: boolean }) {
  const tr = useTr()
  const screen = store.screen
  if (!available) {
    const [icon, text] = store.connection.kind !== 'online'
      ? ['wifi.slash', tr('This computer is offline.')]
      : !screen
        ? ['arrow.down.circle', tr('Update Codync on your computer to use its screen from here.')]
        : ['lock.display', screenGuidance(screen, tr)]
    return <Notice icon={icon} text={text} />
  }
  const phase = session?.phase
  if (!session || !phase || phase.kind === 'connecting') return <Notice text={tr('Connecting to the screen…')} />
  if (phase.kind === 'reconnecting') return <Notice text={tr('Reconnecting…')} />
  if (phase.kind === 'failed') return <Notice icon="exclamationmark.triangle" text={tr(phase.message)} retry={() => void session.start()} />
  const note = !canControl ? tr('View only. To allow control, open Codync → Settings → Computer access on the computer.') : session.lastError ? tr(session.lastError) : null
  return note ? <Text className="screen-viewer-note" size="caption" tone="secondary">{note}</Text> : null
}

function Notice({ icon, text, retry }: { icon?: string; text: string; retry?: () => void }) {
  return (
    <VStack gap={12} className="screen-viewer-notice">
      {icon ? <Icon name={icon} size={28} color="var(--secondary)" /> : <Spinner size={22} />}
      <Text size="callout" tone="secondary">{text}</Text>
      {retry ? <Button kind="secondary" onClick={retry} title="Try again"><Icon name="arrow.clockwise" size={13} /></Button> : null}
    </VStack>
  )
}
