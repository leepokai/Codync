import { useState } from 'react'
import { devServerAddress } from '@shared/dev-servers'
import type { Bot } from '@shared/models'
import { previewAddress } from '@shared/panel-layout'
import { IconButton } from '../../components/Controls'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { analytics, useScreenViewed } from '../../store/analytics'
import { useApp, useStore } from '../../store/context'
import './browser-tab.css'

/** The address each bot's browser tab shows, kept while the app runs: a tab left and shown again is where it was. */
const shown = new Map<string, string>()

/**
 * A web page beside the chat: what one of the bot's dev servers serves, or any address typed. Only http and https
 * pages show, each in a sandboxed frame of its own origin that can open no window and can't move the app's page.
 */
// ponytail: a frame, so a site that refuses to be framed stays blank (Open in browser shows it); a view of its own
// in the main process when that matters.
export function BrowserTab({ bot, tab }: { bot: Bot; tab: string }) {
  const tr = useTr()
  const store = useStore()
  const app = useApp()
  const [address, setAddress] = useState(() => shown.get(`${bot.id}/${tab}`) ?? '')
  const [typed, setTyped] = useState(address)
  const [refused, setRefused] = useState(false)
  // Refresh loads the page again in a new frame.
  const [turn, setTurn] = useState(0)
  useScreenViewed('bot_browser')
  const open = (text: string, from: 'address' | 'dev_server') => {
    const next = previewAddress(text, location.origin)
    setRefused(!next)
    if (!next) return
    shown.set(`${bot.id}/${tab}`, next)
    setAddress(next)
    setTyped(next)
    setTurn((t) => t + 1)
    analytics.track({ event: 'preview_opened', from })
  }
  const servers = bot.devServers.flatMap((server) => {
    const at = devServerAddress(server, store === app.local)
    return at ? [{ id: server.id, name: server.name, at }] : []
  })
  return (
    <div className="browser-tab">
      <div className="browser-bar">
        <form className="browser-form" onSubmit={(e) => { e.preventDefault(); open(typed, 'address') }}>
          <input className="browser-address selectable" value={typed} onChange={(e) => setTyped(e.target.value)} placeholder={tr('Web address')} aria-label={tr('Web address')} aria-invalid={refused} spellCheck={false} autoCapitalize="off" />
        </form>
        <IconButton title="Refresh" icon="arrow.clockwise" size={26} disabled={!address} onClick={() => setTurn((t) => t + 1)} />
        <IconButton title="Open in browser" icon="safari" size={26} disabled={!address} onClick={() => window.codync.app.openExternal(address)} />
      </div>
      {refused ? <Text as="div" size="caption" tone="danger" className="browser-note">{tr('That is not a web address')}</Text> : null}
      {address ? (
        <iframe key={turn} className="browser-frame" src={address} title={tr('Preview')} sandbox="allow-scripts allow-same-origin allow-forms" referrerPolicy="no-referrer" />
      ) : (
        <div className="browser-empty">
          <Text as="div" size="body" tone="secondary">{tr('Enter an address to preview a page here')}</Text>
          {servers.length ? (
            <div className="browser-servers">
              <Text as="div" size="caption" weight="semibold" tone="secondary">{tr('Dev servers')}</Text>
              {servers.map((server) => (
                <button key={server.id} className="panel-choice press" onClick={() => open(server.at, 'dev_server')}>
                  <Text size="body" tone="text" grow truncate>{server.name}</Text>
                  <Text size="caption" tone="secondary" tabular>{server.at.replace(/^https?:\/\//, '')}</Text>
                </button>
              ))}
            </div>
          ) : null}
        </div>
      )}
    </div>
  )
}
