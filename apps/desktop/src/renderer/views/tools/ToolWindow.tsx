import { useEffect, useRef, useState } from 'react'
import { TOOLS, type ToolId } from '@shared/tools'
import { Button, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { useTool } from '../../lib/use-tool'
import { useTr } from '../../lib/use-language'
import './tools.css'

const TAIL_LINES = 6

/**
 * A bundled tool's start page, shown while the host brings the tool up: what it is doing, the last
 * lines of its log, and Retry / View log when it fails. Once the host reports it running, the main
 * process loads the tool into this same window. The tool's own page is never shown half started.
 */
export function ToolWindow({ id }: { id: ToolId }) {
  const spec = TOOLS[id]
  const { state, hostKnown, host, setState, call } = useTool(id)
  const [busy, setBusy] = useState(false)
  const [actionError, setActionError] = useState<string | null>(null)
  const startedOnce = useRef(false)
  const tr = useTr()

  const start = () => {
    if (busy) return
    setBusy(true)
    setActionError(null)
    void call('toolStart')
      .then(setState)
      .catch((e: unknown) => setActionError(e instanceof Error ? e.message : String(e)))
      .finally(() => setBusy(false))
  }

  // A Codync built without the tool (Windows ARM64, a quick development package) can only say so.
  const missing = state?.comfy?.bundled === false

  // Opening the window means "bring it up": start it if the host has it stopped.
  useEffect(() => {
    if (state?.state === 'stopped' && !missing && !startedOnce.current) {
      startedOnce.current = true
      start()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state?.state, missing])

  useEffect(() => {
    if (state?.state === 'running') void window.codync.tools.gotoEditor(id).catch(() => {})
  }, [state?.state, id])

  const error = state?.state === 'error' ? (state.error ?? tr('Unknown error')) : actionError
  const tail = (state?.logTail ?? '').split('\n').filter(Boolean).slice(-TAIL_LINES).join('\n')

  return (
    <div className="tool-window">
      <div className="tool-status">
        <div className="tool-icon"><Icon name={spec.icon} size={26} weight="medium" /></div>
        <div className="tool-title">{spec.title}</div>
        {host === null || !hostKnown ? (
          <>
            <Spinner size={20} />
            <p className="tool-detail">{tr("Waiting for this computer's host…")}</p>
          </>
        ) : missing ? (
          <p className="tool-detail">{tr('This build of Codync does not include ComfyUI.')}</p>
        ) : error ? (
          <>
            <p className="tool-detail tool-error">{error}</p>
            <div className="tool-actions">
              <Button onClick={start} disabled={busy}>{busy ? tr('Starting…') : tr('Try again')}</Button>
              <Button kind="secondary" onClick={() => void window.codync.tools.openLog(id)}>{tr('View log')}</Button>
            </div>
          </>
        ) : state === null ? (
          <>
            <Spinner size={20} />
            <p className="tool-detail">{tr('Checking {name}…').replace('{name}', spec.title)}</p>
          </>
        ) : state.state === 'running' ? (
          <>
            <Spinner size={20} />
            <p className="tool-detail">{tr('Opening the editor…')}</p>
          </>
        ) : (
          <>
            <Spinner size={20} />
            <p className="tool-detail">{tr('Starting {name}… the first start can take a few minutes.').replace('{name}', spec.title)}</p>
            {tail ? <pre className="tool-log">{tail}</pre> : null}
            <div className="tool-actions">
              <Button kind="secondary" onClick={() => void window.codync.tools.openLog(id)}>{tr('View log')}</Button>
            </div>
          </>
        )}
      </div>
    </div>
  )
}
