import { IconButton } from '../../components/Controls'
import { analytics } from '../../store/analytics'
import { splitPanel } from './DetailsPanel'

/**
 * The window bar's end for a conversation with a panel beside it, a bot's chat and a thread alike: while the panel is
 * open its own buttons (Create template and Split panel for a bot's panel, `share` given; Maximize while it sits beside
 * the conversation, `setFilled` given), then the panel's toggle, which also closes it.
 */
export function PanelBarButtons({ open, filled, setFilled, share, toggle }: {
  open: boolean
  filled: boolean
  setFilled?: (filled: boolean) => void
  share?: () => void
  toggle: () => void
}) {
  const maximize = setFilled ? () => {
    analytics.track({ event: 'panel_maximized', on: !filled })
    setFilled(!filled)
  } : undefined
  return (
    <>
      {open && share ? <IconButton title="Create template" icon="square.and.arrow.up" size={28} onClick={share} /> : null}
      {open && maximize ? <IconButton title="Maximize panel" icon={filled ? 'arrow.down.right.and.arrow.up.left' : 'arrow.up.left.and.arrow.down.right'} size={28} onClick={maximize} /> : null}
      {open && share ? <IconButton title="Split panel" icon="rectangle.split.1x2" size={28} onClick={splitPanel} /> : null}
      <IconButton title="Conversation details" icon="sidebar.right" size={28} onClick={toggle} />
    </>
  )
}
