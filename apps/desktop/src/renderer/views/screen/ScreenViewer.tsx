import { useEffect, useRef, useState } from 'react'
import { Button, DropdownMenu, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { fill } from '../../lib/fill'
import { useModel } from '../../lib/observable'
import { screenAvailable, screenNotice, type ScreenNotice } from '../../lib/screen-state'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import type { BotStore } from '../../store/bot-store'
import type { ScreenSession } from '../../store/screen-session'
import { useScreenInput } from './use-screen-input'
import { screenComputer, useScreenSession, useVideo, useViewSize } from './use-screen-session'
import './screen-viewer.css'

/**
 * Another computer's screen in a sheet over the window (the iPhone's `ScreenView`): the picture, and the mouse and
 * keyboard over it once you take over from the bots. Opened to watch a bot (`watching`), it starts by looking.
 */
export function ScreenViewer({ store, watching }: { store: BotStore; watching: string | null }) {
  const tr = useTr()
  useModel(store)
  useScreenViewed('remote_screen')
  const screen = store.screen
  const online = store.connection.kind === 'online'
  const available = online && screenAvailable(screen)
  const session = useScreenSession(store, available)
  const [interactive, setInteractive] = useState(false)
  const tookOver = useRef(false)
  /** Whether the user wants the controls: at first unless this opened to watch a bot, then what they last chose. */
  const wanted = useRef(!watching)
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
  const choose = (on: boolean) => {
    wanted.current = on
    control(on)
  }
  // Each session (a new one replaces it after an outage) and control allowed again bring the user's choice back.
  useEffect(() => {
    if (!canControl) control(false)
    else if (session) control(wanted.current)
  }, [session, canControl])
  // Back live after a failure or a long outage, the host may have handed control back to the bots when its last
  // viewer went: ask again.
  const live = session?.phase.kind === 'live'
  useEffect(() => {
    if (live && tookOver.current && store.screen?.userControl === false) store.screenTakeover(true)
  }, [live])
  useEffect(() => () => {
    if (tookOver.current) store.screenTakeover(false)
  }, [store])

  useVideo(video, session)
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
      <IconButton title={interactive ? 'Hand back to bots' : 'Take over'} icon={interactive ? 'hand.raised.slash' : 'hand.raised'} selected={interactive} disabled={!canControl} onClick={() => choose(!interactive)} />
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

function ScreenStatus({ store, session, available, canControl }: { store: BotStore; session: ScreenSession | null; available: boolean; canControl: boolean }) {
  const tr = useTr()
  const notice = screenNotice(screenComputer(store), session, available, tr)
  if (notice) return <Notice {...notice} />
  const note = !canControl ? tr('View only. To allow control, open Codync → Settings → Computer access on the computer.') : session?.lastError ? tr(session.lastError) : null
  return note ? <Text className="screen-viewer-note" size="caption" tone="secondary">{note}</Text> : null
}

/** What shows instead of the picture; `compact` in a small picture (the bot details' Computer tab). */
export function Notice({ icon, text, retry, compact = false }: ScreenNotice & { compact?: boolean }) {
  return (
    <VStack gap={compact ? 8 : 12} className={`screen-viewer-notice${compact ? ' compact' : ''}`}>
      {icon ? <Icon name={icon} size={compact ? 20 : 28} color="var(--secondary)" /> : <Spinner size={compact ? 18 : 22} />}
      <Text size={compact ? 'caption' : 'callout'} tone="secondary">{text}</Text>
      {retry ? <Button kind="secondary" onClick={retry} title="Try again"><Icon name="arrow.clockwise" size={13} /></Button> : null}
    </VStack>
  )
}
