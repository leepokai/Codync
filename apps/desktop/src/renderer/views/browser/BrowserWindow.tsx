import { useCallback, useEffect, useRef, useState } from 'react'
import { Button, IconButton } from '../../components/Controls'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useApp } from '../../store/context'
import './browser-window.css'

interface Tab {
  id: string
  title: string
  url: string
}

interface Panel {
  open: boolean
  activeId?: string
  tabs: Tab[]
}

const label = (tab: Tab) => tab.title.trim() || tab.url.replace(/^https?:\/\//, '').replace(/\/$/, '')

/** The host's browser, one tab at a time: a picture of the page, the tab strip and the address bar. */
export function BrowserWindow() {
  const tr = useTr()
  const app = useApp()
  const [panel, setPanel] = useState<Panel>({ open: false, tabs: [] })
  const [image, setImage] = useState<string | null>(null)
  const [address, setAddress] = useState('')
  const [compact, setCompact] = useState(() => localStorage.getItem('codync.browser.compact') === '1')
  const toggleCompact = () => {
    setCompact((on) => {
      localStorage.setItem('codync.browser.compact', on ? '0' : '1')
      return !on
    })
  }
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const active = useRef<string | null>(null)
  active.current = panel.activeId ?? null
  // The callbacks read these through refs, so they stay stable while the page renders.
  const appRef = useRef(app)
  appRef.current = app
  const trRef = useRef(tr)
  trRef.current = tr
  const started = useRef(false)
  const client = async () => {
    const local = appRef.current.local
    return local ? await local.ready() : null
  }
  const fail = (reason: unknown) => setError(reason instanceof Error ? reason.message : String(reason))
  const call = useCallback(async (method: string, params: object = {}) => {
    const c = await client()
    if (!c) throw new Error(trRef.current('This computer’s host is not answering.'))
    return c.call<Panel>(method, params)
  }, [])

  const load = useCallback(async () => {
    try {
      setPanel(await call('browserPanel'))
    } catch (reason) {
      fail(reason)
    }
  }, [call])

  const shoot = useCallback(async () => {
    const tab = active.current
    if (!tab) return setImage(null)
    try {
      const c = await client()
      if (!c) return
      setImage((await c.call<{ image: string }>('browserPanelShot', { tabId: tab })).image)
    } catch (reason) {
      // A page still loading gives no picture; the next tick tries again.
      fail(reason)
    }
  }, [])

  useEffect(() => {
    if (started.current) return
    started.current = true
    void (async () => {
      try {
        setPanel(await call('browserPanelOpen'))
      } catch (reason) {
        fail(reason)
      }
    })()
  }, [call])

  useEffect(() => {
    const timer = setInterval(() => void load(), 3_000)
    return () => clearInterval(timer)
  }, [load])

  useEffect(() => {
    const timer = setInterval(() => void shoot(), 2_000)
    return () => clearInterval(timer)
  }, [shoot])
  useEffect(() => {
    void shoot()
  }, [panel.activeId, shoot])

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null
    const place = () => {
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => {
        void (async () => {
          const c = await client()
          if (!c) return
          const width = Math.max(480, Math.round(window.screen.availWidth / 2))
          const height = Math.max(420, window.screen.availHeight - 80)
          try {
            await c.call('browserPanelPlace', { x: window.screen.availWidth - width, y: 40, width, height })
          } catch {
            // A host without placement keeps the browser where Chrome put it.
          }
        })()
      }, 150)
    }
    place()
    window.addEventListener('resize', place)
    return () => {
      window.removeEventListener('resize', place)
      if (timer) clearTimeout(timer)
    }
  }, [client])

  const run = async (method: string, params: object = {}) => {
    setBusy(true)
    setError(null)
    try {
      setPanel(await call(method, params))
      await shoot()
    } catch (reason) {
      fail(reason)
    }
    setBusy(false)
  }

  const openTab = async () => {
    const url = address.trim()
    if (!url) return
    await run('browserPanelOpen', { url })
    setAddress('')
  }

  const navigate = async () => {
    const tab = panel.activeId
    if (!tab || !address.trim()) return
    await run('browserPanelNavigate', { tabId: tab, url: address.trim() })
    setAddress('')
  }

  return (
    <div className="browser-window">
      <header className="browser-tabs">
        <HStack gap={4} className="browser-tab-strip">
          {panel.tabs.map((tab) => (
            <div key={tab.id} className="browser-tab" data-current={tab.id === panel.activeId ? 'true' : undefined}>
              <button type="button" className="browser-tab-name" title={tab.url} onClick={() => void run('browserPanelActivate', { tabId: tab.id })}>
                {label(tab) || tr('New tab')}
              </button>
              <IconButton title={tr('Close tab')} icon="xmark" size={16} disabled={busy || panel.tabs.length === 1}
                onClick={() => void run('browserPanelClose', { tabId: tab.id })} />
            </div>
          ))}
          <IconButton title={tr('New tab')} icon="plus" size={16} disabled={busy}
            onClick={() => void run('browserPanelOpen', { url: address.trim() ? address.trim() : 'about:blank' }).then(() => setAddress(''))} />
          <IconButton title={tr('Close all tabs')} icon="trash" size={16} disabled={busy || !panel.open}
            onClick={() => void run('browserPanelShutdown')} />
        </HStack>
        <HStack gap={6} className="browser-address">
          <input className="field-box" value={address} spellCheck={false} aria-label={tr('Address')}
            placeholder={tr('Paste an address, or type one')} onChange={(e) => setAddress(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') void (panel.activeId ? navigate() : openTab()) }} />
          <Button kind="secondary" disabled={busy || !address.trim()}
            onClick={() => void (panel.activeId ? navigate() : openTab())}>{tr('Go')}</Button>
          <Button kind="secondary" disabled={busy || !panel.activeId} onClick={() => void run('browserPanelShot', { tabId: panel.activeId })}>{tr('Refresh')}</Button>
          <IconButton title={tr(compact ? 'Show the page' : 'Hide the page')} icon={compact ? 'rectangle.expand.vertical' : 'rectangle.compress.vertical'} size={16} onClick={toggleCompact} />
        </HStack>
      </header>
      {error ? <Text size="caption" role="alert" tone="danger" className="browser-error">{error}</Text> : null}
      {compact ? null : <div className="browser-page">
        {image ? <img src={image} alt={tr('Browser page')} draggable={false} /> : (
          <Text tone="secondary">{panel.open ? tr('Loading the page…') : tr('Opening the browser…')}</Text>
        )}
      </div>}
      <footer>
        <Text size="caption" tone="tertiary">{tr('Type and click in its own window; this panel shows it and keeps its tabs.')}</Text>
      </footer>
    </div>
  )
}
