import { useEffect, useRef, useState } from 'react'
import type { HostSnapshot } from '@shared/ipc'
import type { N8nState } from '@shared/models'
import { Button, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { useTr } from '../../lib/use-language'
import './n8n.css'

const POLL_MS = 1500

/**
 * The n8n window's own page, shown while the host brings the n8n instance up.
 * Once it reports running, the main process loads the editor into this same window.
 */
export function N8nWindow() {
  const [host, setHost] = useState<HostSnapshot | null>(null)
  const [state, setState] = useState<N8nState | null>(null)
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
        .host.call(local.baseURL, local.token, 'n8nStatus', {}, 5000)
        .then((s) => alive && setState(s as N8nState))
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
      .host.call(local.baseURL, local.token, 'n8nStart', {}, 15_000)
      .then((s) => setState(s as N8nState))
      .catch((e: unknown) => setActionError(e instanceof Error ? e.message : String(e)))
      .finally(() => setStarting(false))
  }

  // Opening the window means "bring it up": start it if the host has it stopped.
  useEffect(() => {
    if (state?.state === 'stopped' && state.path && !startedOnce.current) {
      startedOnce.current = true
      start()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state?.state, state?.path, host?.local])

  // Running: hand the window over to the editor.
  useEffect(() => {
    if (state?.state === 'running') void window.codync.n8n.gotoEditor().catch(() => {})
  }, [state?.state])

  const noHost = host === null || host.local === null
  const error = state?.state === 'error' ? (state.error ?? tr('Unknown error')) : actionError
  const needsFolder = state !== null && !state.path
  const openSettings = () => window.codync.n8n.openSettings()

  return (
    <div className="n8n-window">
      <div className="n8n-status">
        <div className="n8n-icon">
          <Icon name="point.3.connected.trianglepath.dotted" size={26} weight="medium" />
        </div>
        <div className="n8n-title">n8n</div>
        {noHost ? (
          <>
            <Spinner size={20} />
            <p className="n8n-detail">{tr("Waiting for this computer's host…")}</p>
          </>
        ) : state === null ? (
          <>
            <Spinner size={20} />
            <p className="n8n-detail">{tr('Checking n8n…')}</p>
          </>
        ) : state.state === 'running' ? (
          <>
            <Spinner size={20} />
            <p className="n8n-detail">{tr('Opening the editor…')}</p>
          </>
        ) : state.state === 'starting' ? (
          <>
            <Spinner size={20} />
            <p className="n8n-detail">{tr('Starting n8n… the first start can take a minute.')}</p>
          </>
        ) : error ? (
          <>
            <p className="n8n-detail n8n-error">{error}</p>
            <div className="n8n-actions">
              {needsFolder ? (
                <Button onClick={openSettings}>{tr('Choose the n8n folder')}</Button>
              ) : (
                <>
                  <Button onClick={start} disabled={starting}>
                    {starting ? tr('Starting…') : tr('Try again')}
                  </Button>
                  <Button kind="secondary" onClick={openSettings}>
                    {tr('Open settings')}
                  </Button>
                </>
              )}
            </div>
          </>
        ) : (
          <>
            <p className="n8n-detail">{needsFolder ? tr('Choose where n8n is installed to start it.') : tr('n8n is not running.')}</p>
            <div className="n8n-actions">
              {needsFolder ? (
                <Button onClick={openSettings}>{tr('Choose the n8n folder')}</Button>
              ) : (
                <Button onClick={start} disabled={starting}>
                  {starting ? tr('Starting…') : tr('Start')}
                </Button>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  )
}
