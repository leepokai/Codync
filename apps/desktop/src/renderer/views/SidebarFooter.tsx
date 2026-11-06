import { useLayoutEffect, useRef } from 'react'
import { IconButton } from '../components/Controls'
import { Icon } from '../components/Icon'
import { Text } from '../components/Text'
import type { AccountMismatch } from '../lib/auto-claim'
import { useModel } from '../lib/observable'
import { useTr } from '../lib/use-language'
import { useAccount } from '../store/account'
import { useApp } from '../store/context'
import { ProfileAvatar } from './PanelMenus'
import { UpdateBadge } from './UpdateButton'
import gmail from '../assets/gmail.png'
import calendar from '../assets/google-calendar.png'
import drive from '../assets/google-drive.png'
import './sidebar-footer.css'

/** Account and app connections float above the scrolling roster; Extensions only when given. */
export function SidebarFooter({ accountOpen, onAccount, onExtensions, onComputers }: {
  accountOpen: boolean
  onAccount: () => void
  onExtensions?: () => void
  /** Settings → Computers, where a computer out of the account is added. */
  onComputers: () => void
}) {
  const tr = useTr()
  const notice = useAccountNotice(onComputers)
  const footer = useRef<HTMLDivElement>(null)
  // The roster scrolls under the footer: its height (taller with the notice, wrapped or not) is the roster's bottom padding.
  useLayoutEffect(() => {
    const el = footer.current
    const sidebar = el?.parentElement
    if (!el || !sidebar) return
    const observer = new ResizeObserver(([e]) => sidebar.style.setProperty('--sidebar-footer-height', `${Math.ceil(e!.borderBoxSize[0]!.blockSize)}px`))
    observer.observe(el)
    return () => {
      observer.disconnect()
      sidebar.style.removeProperty('--sidebar-footer-height')
    }
  }, [])
  return (
    <div className="sidebar-footer" ref={footer}>
      <div className="sidebar-footer-fade" aria-hidden />
      {notice ? (
        // This computer and the account signed in here differ: a notice in full, the choice behind it.
        <button className="sidebar-account-notice" onClick={notice.open}>
          <Icon name="exclamationmark.circle.fill" size={13} color="var(--attention)" />
          <Text size="caption" grow>{notice.text}</Text>
          <Icon name="chevron.right" size={10} color="var(--tertiary)" />
        </button>
      ) : null}
      <button
        className={`sidebar-account ${accountOpen ? 'active' : ''}`}
        aria-label="Open account menu for Account"
        aria-expanded={accountOpen}
        title="Account menu"
        onClick={onAccount}
      >
        <ProfileAvatar />
      </button>
      {onExtensions ? (
        <button className="extensions" onClick={onExtensions}>
          <span>{tr('Extensions')}</span>
          <span className="extensions-logos" aria-hidden>
            {[gmail, calendar, drive].map((src) => <img key={src} src={src} alt="" draggable={false} />)}
          </span>
        </button>
      ) : null}
      <UpdateBadge size={34} />
    </div>
  )
}

/** The narrow rail's notice: an icon with the notice's text as its tooltip, the same choice behind it. */
export function RailAccountNotice({ onComputers }: { onComputers: () => void }) {
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
