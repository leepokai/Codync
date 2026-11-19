import type { ReactNode } from 'react'
import { IconButton } from '../components/Controls'
import { Text } from '../components/Text'
import type { AccountMismatch } from '../lib/join-offer'
import { useModel } from '../lib/observable'
import { useTr } from '../lib/use-language'
import { useAccount } from '../store/account'
import { useApp } from '../store/context'
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
      <RailAccountNotice />
      <button className={`rail-profile ${accountOpen ? 'open' : ''}`} aria-label={tr('Open account menu for Account')} title={tr('Account')} onClick={onAccount}>
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

/** The rail has no room for the sentence: the same notice as a tinted icon, its text in the tooltip. */
function RailAccountNotice() {
  const notice = useAccountNotice()
  return notice ? <IconButton title={notice.text} icon="exclamationmark.circle.fill" size={36} className="rail-account-notice" onClick={notice.open} /> : null
}

/** This computer is in another account than the one signed in here (`CloudModel.mismatch`): ask again about moving it. */
function useAccountNotice() {
  const tr = useTr()
  const app = useApp()
  useAccount()
  useModel(app.cloud)
  useModel(app.local)
  const mismatch = app.cloud.mismatch
  if (!mismatch) return null
  return { text: mismatchLine(mismatch, tr), open: () => app.cloud.reconsiderMove() }
}

function mismatchLine(mismatch: AccountMismatch, tr: (text: string) => string) {
  return mismatch.email
    ? tr(`This computer is in {name}'s account`).replaceAll('{name}', mismatch.email)
    : tr('This computer is in another account')
}
