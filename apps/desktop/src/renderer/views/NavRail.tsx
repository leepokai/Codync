import { createContext, useRef, useState } from 'react'
import { fill } from '@shared/fill'
import { IconButton } from '../components/Controls'
import { Icon } from '../components/Icon'
import { AnchoredMenu, type MenuItem } from '../components/Menu'
import { useModel } from '../lib/observable'
import { useTr } from '../lib/use-language'
import { useAccount } from '../store/account'
import { useApp } from '../store/context'
import { ProfileAvatar } from './PanelMenus'
import { UpdateBadge } from './UpdateButton'
import type { ExtensionFocus } from './marketplace/market-models'
import './nav-rail.css'

/** What the chat window shows: one entry of the rail each. */
/** Opens Extensions on a computer, from deep in a view (the model panel's add agent, a bot's Add login profile…),
 * optionally on one tab and agent. */
export const OpenExtensions = createContext<((computerId: string, focus?: ExtensionFocus) => void) | null>(null)

export type AppPage = 'bots' | 'threads' | 'workflows' | 'computers' | 'network' | 'extensions' | 'usage' | 'settings'

const ENTRIES: { id: AppPage; title: string; icon: string }[] = [
  { id: 'bots', title: 'Bots', icon: 'bot' },
  { id: 'threads', title: 'Threads', icon: 'checklist' },
  { id: 'workflows', title: 'Workflows', icon: 'flowchart' },
  { id: 'computers', title: 'Computers', icon: 'desktopcomputer' },
  { id: 'network', title: 'Network', icon: 'codync.network' },
  { id: 'extensions', title: 'Extensions', icon: 'square.grid.2x2' },
  { id: 'usage', title: 'Usage', icon: 'gauge.with.dots.needle.33percent' },
]

/** A page's rail icon, also shown beside its list's title. */
export const pageIcon = (page: AppPage) => ENTRIES.find((entry) => entry.id === page)?.icon ?? 'gearshape'

/**
 * The window's pages down its leading edge, icons only: Bots, Threads, Workflows (`workflows`: while a computer
 * offers them), Computers, Network, Extensions and Usage,
 * then an available update, the account notice, Settings and the account's avatar, which opens the account's menu. `working` counts the
 * threads at work (on Threads) and `approvals` the devices waiting to be let in (on Computers).
 */
export function NavRail({ page, go, working, approvals, workflows }: { page: AppPage; go: (page: AppPage) => void; working: number; approvals: number; workflows: boolean }) {
  const tr = useTr()
  const counts: Partial<Record<AppPage, number>> = { threads: working, computers: approvals }
  return (
    <nav className="nav-rail drag">
      {ENTRIES.filter((entry) => workflows || entry.id !== 'workflows').map(({ id, title, icon }) => (
        <button key={id} data-page={id} className={`nav-rail-entry no-drag ${id === page ? 'selected' : ''}`} title={tr(title)} aria-label={tr(title)} aria-current={id === page ? 'page' : undefined} onClick={() => go(id)}>
          <Icon name={icon} size={16} weight="medium" />
          {counts[id] ? <span className="nav-rail-count" aria-hidden>{counts[id]}</span> : null}
        </button>
      ))}
      <span className="nav-rail-space" />
      <AccountNotice />
      <UpdateBadge size={32} />
      <button data-page="settings" className={`nav-rail-entry no-drag ${page === 'settings' ? 'selected' : ''}`} title={tr('Settings')} aria-label={tr('Settings')} aria-current={page === 'settings' ? 'page' : undefined} onClick={() => go('settings')}>
        <Icon name="gearshape" size={16} weight="medium" />
      </button>
      <AccountButton />
    </nav>
  )
}

/** The account's avatar: who is signed in, with signing out and deleting the account, or the ways to sign in. */
function AccountButton() {
  const tr = useTr()
  const account = useAccount()
  const button = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  const items = (): MenuItem[] =>
    account.isSignedIn
      ? [
          { title: account.user?.email ?? 'Signed in', verbatim: !!account.user?.email, disabled: true, action: () => {} },
          { title: 'Sign out', icon: 'rectangle.portrait.and.arrow.right', divider: true, action: () => void account.signOut() },
          { title: 'Delete account', icon: 'trash', destructive: true, action: () => account.askToDelete() },
        ]
      : [
          { title: 'Not signed in', disabled: true, action: () => {} },
          { title: 'Continue with Apple', divider: true, action: () => void account.signIn('apple') },
          { title: 'Continue with Google', action: () => void account.signIn('google') },
          { title: 'Continue with GitHub', action: () => void account.signIn('github') },
        ]
  return (
    <>
      <button ref={button} className={`nav-rail-entry no-drag ${open ? 'selected' : ''}`} title={tr('Account')} aria-label={tr('Account')} aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <ProfileAvatar />
      </button>
      <AnchoredMenu open={open} onClose={() => setOpen(false)} anchor={button} items={items} />
    </>
  )
}

/**
 * This computer is in another account than the one signed in here (`CloudModel.mismatch`): a tinted icon with the
 * notice as its tooltip, which asks about the move again.
 */
function AccountNotice() {
  const tr = useTr()
  const app = useApp()
  useAccount()
  useModel(app.cloud)
  useModel(app.local)
  const mismatch = app.cloud.mismatch
  if (!mismatch) return null
  const text = mismatch.email ? fill(tr(`This computer is in {name}'s account`), { name: mismatch.email }) : tr('This computer is in another account')
  return <IconButton title={text} icon="exclamationmark.circle.fill" size={36} className="nav-rail-notice no-drag" onClick={() => app.cloud.reconsiderMove()} />
}
