import type { ReactNode } from 'react'
import { IconButton } from '../components/Controls'
import { Text } from '../components/Text'
import type { AccountMismatch } from '../lib/auto-claim'
import { useModel } from '../lib/observable'
import { useTr } from '../lib/use-language'
import { useAccount } from '../store/account'
import { useApp } from '../store/context'
import { ProfileAvatar } from './PanelMenus'
import { UpdateBadge } from './UpdateButton'

/** The icon rail at the left of the window. The bot list opens from the agents button. */
export function PageRail({ open, workflowsOpen, onToggle, accountOpen, onAccount, onWorkflows, onExtensions, onComputers }: {
  open: boolean
  workflowsOpen: boolean
  onToggle: () => void
  accountOpen: boolean
  onAccount: () => void
  onWorkflows?: () => void
  onExtensions: () => void
  /** Settings → Computers, where a computer out of the account is added. */
  onComputers: () => void
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
      <RailAccountNotice onComputers={onComputers} />
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

/** The rail has no room for the sentence: the same notice as a tinted icon, its text in the tooltip. */
function RailAccountNotice({ onComputers }: { onComputers: () => void }) {
  const notice = useAccountNotice(onComputers)
  return notice ? <IconButton title={notice.text} icon="exclamationmark.circle.fill" size={36} className="rail-account-notice" onClick={notice.open} /> : null
}

/**
 * This computer and the account signed in here differ (`CloudModel.mismatch`): what to say, and the choice behind it.
 * In another account, the move prompt again; in none, Settings → Computers.
 */
function useAccountNotice(onComputers: () => void) {
  const tr = useTr()
  const app = useApp()
  useAccount()
  useModel(app.cloud)
  useModel(app.local)
  const mismatch = app.cloud.mismatch
  if (!mismatch) return null
  return { text: mismatchLine(mismatch, tr), open: () => (mismatch.kind === 'other' ? app.cloud.reconsiderMove() : onComputers()) }
}

function mismatchLine(mismatch: AccountMismatch, tr: (text: string) => string) {
  if (mismatch.kind === 'other') {
    return mismatch.email ? tr(`This computer is in {name}'s account`).replaceAll('{name}', mismatch.email) : tr('This computer is in another account')
  }
  switch (mismatch.reason) {
    case 'cloudOff': return tr(`This computer isn't in an account: its cloud is off`)
    case 'privateRelay': return tr(`This computer isn't in an account: it uses a private relay`)
    case 'keptOut': return tr(`This computer isn't in your account`)
  }
}
