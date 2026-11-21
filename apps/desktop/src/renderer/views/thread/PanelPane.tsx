import type { KeyboardEvent, ReactNode } from 'react'
import { fill } from '@shared/fill'
import type { PanelPane as Pane } from '@shared/panel-layout'
import { IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'

/** A view a pane can show: its tab's name, its symbol in the chooser, and the key that opens it there. */
export interface PaneView {
  id: string
  label: string
  icon: string
  key: string
}

/**
 * One pane of the panel beside a conversation: its views as tabs, each with its own close button (more than fit
 * scroll sideways), then `+`, which shows the chooser in place of a view: every view of the bot, opened by a click or
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
  const onKey = (e: KeyboardEvent) => {
    if (!choosing || e.metaKey || e.ctrlKey || e.altKey || (e.target as HTMLElement).closest('input, textarea, [contenteditable]')) return
    const view = views.find((v) => v.key === e.key.toLowerCase())
    if (!view) return
    e.preventDefault()
    show(view.id)
  }
  return (
    <section className="panel-pane" onKeyDown={onKey}>
      <div className="panel-tabs">
        <div className="panel-tabs-row" role="tablist" aria-label={tr('Conversation details')}>
          {pane.tabs.flatMap((id) => views.find((v) => v.id === id) ?? []).map((v) => (
            <span key={v.id} className={v.id === pane.shown ? 'panel-tab on' : 'panel-tab'}>
              <button role="tab" aria-selected={v.id === pane.shown} className="panel-tab-pick" onClick={() => show(v.id)}>{tr(v.label)}</button>
              <button className="panel-tab-close" title={fill(tr('Close {name}'), { name: tr(v.label) })} aria-label={fill(tr('Close {name}'), { name: tr(v.label) })} onClick={() => close(v.id)}>
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
            {views.map((v) => (
              <button key={v.id} className="panel-choice press" onClick={() => show(v.id)}>
                <span className="panel-choice-icon"><Icon name={v.icon} size={15} color="var(--secondary)" /></span>
                <Text size="body" tone="text" grow>{tr(v.label)}</Text>
                <kbd className="panel-key">{v.key.toUpperCase()}</kbd>
              </button>
            ))}
          </div>
        ) : children}
      </div>
    </section>
  )
}
