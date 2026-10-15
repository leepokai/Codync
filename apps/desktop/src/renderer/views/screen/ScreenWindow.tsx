import { useEffect, useRef, useState } from 'react'
import type { ScreenView } from '@shared/screen-viewer'
import { Spinner } from '../../components/Controls'
import { font } from '../../lib/fonts'
import './screen.css'

type Phase = 'connecting' | 'live' | 'closed'

const VNC_HINT = "Can't reach VNC on that computer. It needs a server on 127.0.0.1:5900, plain RFB, with no password."

/** A computer's live screen. noVNC sends clicks, drags, scrolling and keys as RFB input. */
export function ScreenWindow() {
  const [view, setView] = useState<ScreenView | null | undefined>(undefined)
  const [attempt, setAttempt] = useState(0)
  const [phase, setPhase] = useState<Phase>('connecting')
  const [detail, setDetail] = useState<string | null>(null)
  const stageRef = useRef<HTMLDivElement>(null)
  const computerId = new URLSearchParams(location.search).get('computer') ?? ''

  useEffect(() => {
    let gone = false
    void window.codync.screen.target(computerId).then((next) => {
      if (!gone) setView(next)
    })
    return () => {
      gone = true
    }
  }, [computerId])

  useEffect(() => {
    const stage = stageRef.current
    if (!view || !stage) return
    let closed = false
    let client: { disconnect: () => void; scaleViewport: boolean; focusOnClick: boolean; background: string; addEventListener: (name: string, fn: (event: Event) => void) => void }
    let deadline: ReturnType<typeof setTimeout> | undefined
    setPhase('connecting')
    setDetail(null)
    void import('@novnc/novnc').then((mod) => {
      if (closed) return
      const RFB = mod.default
      const rfb = new RFB(stage, view.url, { credentials: { password: '' } })
      client = rfb
      rfb.scaleViewport = true
      rfb.focusOnClick = true
      rfb.background = '#000000'
      rfb.addEventListener('connect', () => {
        if (closed) return
        if (deadline) clearTimeout(deadline)
        setPhase('live')
        setDetail(null)
      })
      rfb.addEventListener('disconnect', () => {
        if (closed) return
        if (deadline) clearTimeout(deadline)
        setPhase('closed')
        setDetail(VNC_HINT)
      })
      deadline = setTimeout(() => {
        if (closed) return
        rfb.disconnect()
        setPhase('closed')
        setDetail(VNC_HINT)
      }, 15_000)
    }).catch(() => {
      if (!closed) {
        setPhase('closed')
        setDetail("Couldn't start the viewer.")
      }
    })
    return () => {
      closed = true
      if (deadline) clearTimeout(deadline)
      client?.disconnect()
      stage.replaceChildren()
    }
  }, [view, attempt])

  const title = view?.name ?? 'Screen'
  const live = phase === 'live'
  return (
    <div className="screen-window">
      <header className="screen-bar">
        <span style={{ ...font(13, 'medium'), minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{title}</span>
        <span style={{ ...font(12), color: 'var(--secondary)', flex: 'none' }}>{live ? 'Live' : phase === 'closed' ? 'Disconnected' : 'Connecting…'}</span>
        <span style={{ flex: 1 }} />
        <span style={{ ...font(11), color: 'var(--tertiary)' }}>noVNC</span>
        {phase === 'closed' ? (
          <button type="button" className="screen-display press" onClick={() => setAttempt((n) => n + 1)}>Retry</button>
        ) : null}
      </header>
      <div className="screen-stage">
        <div ref={stageRef} className="screen-rfb" />
        {live ? null : (
          <div className="screen-status">
            {phase === 'closed' ? null : <Spinner size={22} />}
            <span style={font(13)}>{view === null ? 'This screen has no computer.' : (detail ?? 'Connecting…')}</span>
          </div>
        )}
      </div>
    </div>
  )
}
