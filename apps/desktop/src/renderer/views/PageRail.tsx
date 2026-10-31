import type { ReactNode } from 'react'
import { IconButton } from '../components/Controls'
import { Text } from '../components/Text'
import { useTr } from '../lib/use-language'
import { ProfileAvatar } from './PanelMenus'
import { UpdateBadge } from './UpdateButton'

/** The icon rail at the left of the window. The bot list opens from the agents button. */
export function PageRail({ open, workflowsOpen, onToggle, accountOpen, onAccount, onWorkflows, onExtensions }: {
  open: boolean
  workflowsOpen: boolean
  onToggle: () => void
  accountOpen: boolean
  onAccount: () => void
  onWorkflows?: () => void
  onExtensions: () => void
}) {
  const tr = useTr()
  return (
    <nav className="page-rail" aria-label={tr('Agents')}>
      <RailTip label="Agents">
        <IconButton title="Agents" systemTip={false} icon="bot" size={40} selected={open} onClick={onToggle} />
      </RailTip>
      <div className="page-rail-gap" />
      {onWorkflows ? (
        <RailTip label="Workflows">
          <IconButton title="Workflows" systemTip={false} icon="flowchart" size={36} selected={workflowsOpen} onClick={onWorkflows} />
        </RailTip>
      ) : null}
      <RailTip label="Extensions">
        <IconButton title="Extensions" systemTip={false} icon="square.grid.2x2" size={36} onClick={onExtensions} />
      </RailTip>
      <UpdateBadge size={32} />
      <button className={`rail-profile ${accountOpen ? 'open' : ''}`} aria-label="Open account menu for Account" title="Account" onClick={onAccount}>
        <ProfileAvatar />
      </button>
    </nav>
  )
}

function RailTip({ label, children }: { label: string; children: ReactNode }) {
  const tr = useTr()
  return (
    <span className="page-rail-tip">
      {children}
      <span className="page-rail-tip-bubble" role="tooltip" aria-hidden>
        <Text size="caption" weight="medium">{tr(label)}</Text>
      </span>
    </span>
  )
}
