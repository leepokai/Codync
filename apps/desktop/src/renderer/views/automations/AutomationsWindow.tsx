import { useEffect, useRef, useState } from 'react'
import type { HostSnapshot } from '@shared/ipc'
import type { AutomanState } from '@shared/models'
import { Button, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { useTr } from '../../lib/use-language'
import './automations.css'

const POLL_MS = 1500

/**
 * The Automations window's own page, shown while the host brings the n8n instance up.
 * Once it reports running, the main process loads the editor into this same window.
 */
export function AutomationsWindow() {
  const [host, setHost] = useState<HostSnapshot | null>(null)
  const [state, setState] = useState<AutomanState | null>(null)
  const [starting, setStarting] = useState(false)
  const [actionError, setActionError] = useState<string | null>(null)
  const startedOnce = useRef(false)
  const tr = useTr()

  useEffect(() => {
    let alive = true
    void window.codync.host.snapshot().then((s) => alive && setHost(s))
    const off = window.codync.host.onChange((s) => alive && setHost(s))
    return () => {
      alive = false
      off()
    }
  }, [])

  // The host's state changes as the instance comes up; poll while this page shows.
  useEffect(() => {
    const local = host?.local
    if (!local) return
    let alive = true
    const load = () =>
      void window.codync
        .host.call(local.baseURL, local.token, 'automanStatus', {}, 5000)
        .then((s) => alive && setState(s as AutomanState))
        .catch(() => {})
    load()
    const timer = setInterval(load, POLL_MS)
    return () => {
      alive = false
      clearInterval(timer)
    }
  }, [host?.local])

  const start = () => {
    const local = host?.local
    if (!local || starting) return
    setStarting(true)
    setActionError(null)
    void window.codync
      .host.call(local.baseURL, local.token, 'automanStart', {}, 15_000)
      .then((s) => setState(s as AutomanState))
      .catch((e: unknown) => setActionError(e instanceof Error ? e.message : String(e)))
      .finally(() => setStarting(false))
  }

  // Opening the window means "bring it up": start it if the host has it stopped.
  useEffect(() => {
    if (state?.state === 'stopped' && !startedOnce.current) {
      startedOnce.current = true
      start()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state?.state, host?.local])

  // Running: hand the window over to the editor.
  useEffect(() => {
    if (state?.state === 'running') void window.codync.automan.gotoEditor().catch(() => {})
  }, [state?.state])

  const noHost = host === null || host.local === null
  const error = state?.state === 'error' ? (state.error ?? tr('Unknown error')) : actionError

  return (
    <div className="automan-window">
      <div className="automan-status">
        <div className="automan-icon">
          <Icon name="point.3.connected.trianglepath.dotted" size={26} weight="medium" />
        </div>
        <div className="automan-title">Automations</div>
        {noHost ? (
          <>
            <Spinner size={20} />
            <p className="automan-detail">{tr("Waiting for this computer's host…")}</p>
          </>
        ) : state === null ? (
          <>
            <Spinner size={20} />
            <p className="automan-detail">{tr('Checking Automations…')}</p>
          </>
        ) : state.state === 'running' ? (
          <>
            <Spinner size={20} />
            <p className="automan-detail">{tr('Opening the editor…')}</p>
          </>
        ) : state.state === 'starting' ? (
          <>
            <Spinner size={20} />
            <p className="automan-detail">{tr('Starting Automations… the first start can take a minute.')}</p>
          </>
        ) : error ? (
          <>
            <p className="automan-detail automan-error">{error}</p>
            <div className="automan-actions">
              <Button onClick={start} disabled={starting}>
                {starting ? tr('Starting…') : tr('Try again')}
              </Button>
              <Button kind="secondary" onClick={() => window.codync.automan.openSettings()}>
                {tr('Open settings')}
              </Button>
            </div>
          </>
        ) : (
          <>
            <p className="automan-detail">{tr('Automations is not running.')}</p>
            <div className="automan-actions">
              <Button onClick={start} disabled={starting}>
                {starting ? tr('Starting…') : tr('Start')}
              </Button>
            </div>
          </>
        )}
      </div>
    </div>
  )
}
