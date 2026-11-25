import type { CSSProperties, KeyboardEvent, ReactNode } from 'react'
import { fill } from '@shared/fill'
import type { Bot } from '@shared/models'
import { multiTabs, multiView, type PanelPane as Pane } from '@shared/panel-layout'
import { IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Text } from '../../components/Text'
import { BrowserTab } from './BrowserTab'
import { TerminalTab } from './TerminalTab'
import { useTr } from '../../lib/use-language'

/** A view a pane can show: its tab's name, its symbol in the chooser, and the key that opens it there. A thread's tab
 * (`verbatim`: named by its task, its symbol on the tab) has no key and is not in the chooser. */
export interface PaneView {
  id: string
  label: string
  icon: string
  key?: string
  verbatim?: boolean
}

/** The tabs of browsers and terminals among `tabs`, each named after its view (`base`), numbered from the second on
 * ("Terminal 2"), its view's symbol on the tab. */
export function multiTabViews(tabs: readonly string[], base: readonly PaneView[], tr: (s: string) => string): PaneView[] {
  return multiTabs(tabs).flatMap(({ id, view, n }) => {
    const of = base.find((v) => v.id === view)
    return of ? [{ id, label: n > 1 ? `${tr(of.label)} ${n}` : tr(of.label), icon: of.icon, verbatim: true }] : []
  })
}

/** A browser's or a terminal's tab `tab` of `bot`'s panel, its terminal in `thread`'s folder when given. */
export function MultiTab({ bot, thread, tab }: { bot: Bot; thread?: string | null; tab: string }) {
  const view = multiView(tab)
  return view === 'browser' ? <BrowserTab key={`${bot.id}/${tab}`} bot={bot} tab={tab} />
    : view === 'terminal' ? <TerminalTab key={`${thread ?? bot.id}/${tab}`} bot={bot} thread={thread} tab={tab} />
    : null
}

/**
 * One pane of the panel beside a conversation: its views as tabs, each with its own close button, sharing the row's
 * width as a browser's do (a narrow tab shows its close button only while hovered or shown; more than fit at their
 * narrowest scroll sideways), then `+`, which shows the chooser in place of a view: every view of the bot, opened by a click or
 * by its key while the pane has the focus. `flush`: the view shown fills the pane itself (a web page). `first`: the
 * pane the sheet gives the focus to, so the keys scroll it. `actions`: the panel's own buttons, at the end of the row.
 */
export function PanelPane({ pane, views, show, close, choose, flush, first, actions, children }: {
  pane: Pane
  views: readonly PaneView[]
  show: (view: string) => void
  close: (view: string) => void
  choose: () => void
  flush: boolean
  first: boolean
  actions?: ReactNode
  children: ReactNode
}) {
  const tr = useTr()
  const choosing = pane.shown === null
  const choices = views.filter((v) => v.key)
  const name = (v: PaneView) => (v.verbatim ? v.label : tr(v.label))
  const onKey = (e: KeyboardEvent) => {
    if (!choosing || e.metaKey || e.ctrlKey || e.altKey || (e.target as HTMLElement).closest('input, textarea, [contenteditable]')) return
    const view = choices.find((v) => v.key === e.key.toLowerCase())
    if (!view) return
    e.preventDefault()
    show(view.id)
  }
  return (
    <section className="panel-pane" onKeyDown={onKey}>
      <div className="panel-tabs">
        {/* Its tabs share its width (`--tabs` caps it at each one's widest), down to each one's narrowest, then scroll. */}
        <div className="panel-tabs-row" role="tablist" aria-label={tr('Conversation details')} style={{ '--tabs': pane.tabs.length } as CSSProperties}>
          {pane.tabs.flatMap((id) => views.find((v) => v.id === id) ?? []).map((v) => (
            <span key={v.id} className={v.id === pane.shown ? 'panel-tab on' : 'panel-tab'}>
              <button role="tab" aria-selected={v.id === pane.shown} className="panel-tab-pick" title={v.verbatim ? v.label : undefined} onClick={() => show(v.id)}>
                {v.verbatim ? <Icon name={v.icon} size={11} color="var(--secondary)" className="panel-tab-icon" /> : null}
                <span className="panel-tab-name">{name(v)}</span>
              </button>
              <button className="panel-tab-close" title={fill(tr('Close {name}'), { name: name(v) })} aria-label={fill(tr('Close {name}'), { name: name(v) })} onClick={() => close(v.id)}>
                <Icon name="xmark" size={8} weight="bold" />
              </button>
            </span>
          ))}
        </div>
        <IconButton title="Open a view" icon="plus" size={26} onClick={choose} />
        <span className="panel-tabs-space" />
        {actions}
      </div>
      {/* The column is the sheet's scroller, so it takes the focus when the sheet opens: the card around it never
          scrolls, and a first PageDown aimed at the card would move nothing. */}
      <div className={flush && !choosing ? 'details-scroll flush' : 'details-scroll'} tabIndex={-1} data-autofocus={first ? true : undefined}>
        {choosing ? (
          <div className="panel-chooser fade-in">
            <Text as="div" size="body" weight="semibold" tone="text" className="panel-chooser-title">{tr('Open a view')}</Text>
            {choices.map((v) => (
              <button key={v.id} className="panel-choice press" onClick={() => show(v.id)}>
                <span className="panel-choice-icon"><Icon name={v.icon} size={15} color="var(--secondary)" /></span>
                <Text size="body" tone="text" grow>{tr(v.label)}</Text>
                <kbd className="panel-key">{v.key?.toUpperCase()}</kbd>
              </button>
            ))}
          </div>
        ) : children}
      </div>
    </section>
  )
}
