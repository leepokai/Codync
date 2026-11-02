import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Spinner } from '../components/Controls'
import { Icon } from '../components/Icon'
import { IconSlot, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import type { Link } from '@shared/analytics'
import { account, useAccount } from '../store/account'
import { openLink, useScreenViewed } from '../store/analytics'
import googleLogo from '../assets/google.svg'
import { useTr } from '../lib/use-language'


// The chat window's floating panels: the account menu (with its profile avatar) and the roster's
// right-click actions, both built on the same keyboard-driven rows.

export function ProfileAvatar() {
  const { user, isBusy, isReady } = useAccount()
  return (
    <span className="profile-avatar" aria-hidden>
      {isBusy || !isReady ? <Spinner size={14} /> : user?.avatarURL ? (
        <img src={user.avatarURL} alt="" className="profile-avatar-image" />
      ) : (
        <Icon name="person.fill" size={15} weight="medium" color="var(--secondary)" />
      )}
    </span>
  )
}

export interface PanelItem {
  title: string
  icon: string
  detail?: string | null
  /** A bundled image in place of the symbol (the Google logo). */
  image?: string
  chevron?: boolean
  disabled?: boolean
  destructive?: boolean
  /** A name the user gave: shown as typed, never translated. */
  verbatim?: boolean
  action: (anchor: HTMLButtonElement | null) => void
  submenu?: boolean
  expanded?: boolean
}

/** The account menu: a window-local floating surface (no system menu styling). */
export function AccountPanelLayer({ compact, approvals, onDismiss, onUsage, onSettings }: {
  compact: boolean
  approvals: number
  onDismiss: () => void
  onUsage: () => void
  onSettings: () => void
}) {
  const tr = useTr()
  const [page, setPage] = useState<'main' | 'support'>('main')
  const { user, isBusy, errorMessage } = useAccount()
  useScreenViewed('account')
  const signedIn = user !== null
  const auth: PanelItem[] = signedIn
    ? [{ title: isBusy ? 'Please wait…' : 'Sign out', icon: 'rectangle.portrait.and.arrow.right', disabled: isBusy, action: () => { onDismiss(); void account.signOut() } }]
    : [
        { title: 'Continue with Apple', icon: 'apple.logo', disabled: isBusy, action: () => { onDismiss(); void account.signIn('apple') } },
        { title: 'Continue with Google', icon: 'person.crop.circle.badge.plus', image: googleLogo, disabled: isBusy, action: () => { onDismiss(); void account.signIn('google') } },
      ]
  const open = (url: string, link: Link) => {
    onDismiss()
    openLink(url, link)
  }
  const items: PanelItem[] =
    page === 'support'
      ? [
          { title: 'Support', icon: 'chevron.left', action: () => setPage('main') },
          { title: 'Help & documentation', icon: 'book', chevron: true, action: () => open('https://github.com/leepokai/codync#readme', 'docs') },
          { title: 'Report an issue', icon: 'bubble.left', chevron: true, action: () => open('https://github.com/leepokai/codync/issues', 'issues') },
        ]
      : [
          { title: 'Usage', icon: 'gauge.with.dots.needle.33percent', chevron: true, action: onUsage },
          { title: 'Get Codync for mobile', icon: 'iphone', action: () => open('https://apps.apple.com/app/id6760984418', 'app_store') },
          { title: 'Support', icon: 'book.closed', chevron: true, action: () => setPage('support') },
          { title: 'Settings', icon: 'gearshape', detail: approvals > 0 ? String(approvals) : null, action: onSettings },
          ...auth,
        ]
  return (
    <div className="context-layer" onMouseDown={(e) => e.target === e.currentTarget && onDismiss()}>
      <div className={`account-panel ${compact ? 'compact' : ''}`} style={{ width: Math.min(260, window.innerWidth - 32) }}>
        <PanelRows
          items={items}
          onDismiss={onDismiss}
          after={(i) =>
            page === 'main' && i === 3 ? (
              <>
                <div className="panel-divider" />
                <div className="panel-identity">
                  <Icon name="person.crop.circle" size={15} />
                  <Text size="callout" truncate grow>{user?.email ?? tr(`Account`)}</Text>
                  {signedIn ? null : <Text size="caption">{tr(`Not signed in`)}</Text>}
                </div>
              </>
            ) : page === 'support' && i === 0 ? (
              <div className="panel-divider" />
            ) : null
          }
        />
        {page === 'main' && errorMessage ? (
          <Text as="div" size="callout" tone="attention" className="panel-error">{errorMessage}</Text>
        ) : null}
      </div>
    </div>
  )
}

export function PanelRows({ items, onDismiss, after, active = true, label, onPoint, onNavigate, onForward, onBack }: {
  items: PanelItem[]
  onDismiss: () => void
  after?: (i: number) => ReactNode
  active?: boolean
  label?: string
  onPoint?: (index: number, row: HTMLButtonElement) => void
  onNavigate?: () => void
  onForward?: (index: number, row: HTMLButtonElement | null) => void
  onBack?: () => void
}) {
  const tr = useTr()
  const [highlighted, setHighlighted] = useState(0)
  const pointed = useRef(-1)
  const highlightedRow = useRef<HTMLButtonElement>(null)
  // A long list scrolls: ↑/↓ and Tab bring the row Enter would run into view.
  // A block, not an arrow's value: Chromium's scrollIntoView returns a promise, which React would call as the clean-up.
  useEffect(() => {
    if (!active) return
    highlightedRow.current?.focus({ preventScroll: true })
    highlightedRow.current?.scrollIntoView({ block: 'nearest' })
  }, [highlighted, active])
  useEffect(() => {
    if (!active) return
    const onKey = (e: KeyboardEvent) => {
      if (!items.length && e.key !== 'Escape') return
      if (e.key === 'Escape') {
        e.preventDefault()
        onDismiss()
      } else if (e.key === 'ArrowDown' || (e.key === 'Tab' && !e.shiftKey)) {
        e.preventDefault()
        onNavigate?.()
        pointed.current = -1
        setHighlighted((h) => (h + 1) % items.length)
      } else if (e.key === 'ArrowUp' || (e.key === 'Tab' && e.shiftKey)) {
        e.preventDefault()
        onNavigate?.()
        pointed.current = -1
        setHighlighted((h) => (h - 1 + items.length) % items.length)
      } else if (e.key === 'ArrowRight' && onForward) {
        e.preventDefault()
        onForward(highlighted, highlightedRow.current)
      } else if (e.key === 'ArrowLeft' && onBack) {
        e.preventDefault()
        onBack()
      } else if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault()
        const item = items[highlighted]
        if (item && !item.disabled) item.action(highlightedRow.current)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [items, highlighted, onDismiss, active, onNavigate, onForward, onBack])
  return (
    <VStack gap={2} role="menu" aria-label={label} onMouseLeave={() => { pointed.current = -1 }}>
      {items.map((item, i) => (
        <div key={`${i}-${item.title}`} role="none">
          <button
            ref={highlighted === i ? highlightedRow : undefined}
            className={`panel-row ${item.expanded || (active && highlighted === i) ? 'highlight' : ''} ${item.destructive ? 'destructive' : ''}`}
            role="menuitem"
            tabIndex={active && highlighted === i ? 0 : -1}
            aria-haspopup={item.submenu ? 'menu' : undefined}
            aria-expanded={item.submenu ? item.expanded : undefined}
            disabled={item.disabled}
            // Movement, not enter: Chromium re-enters the row under a resting pointer after a scroll, which would undo ↑/↓.
            onMouseMove={(e) => {
              setHighlighted(i)
              if (pointed.current === i) return
              pointed.current = i
              onPoint?.(i, e.currentTarget)
            }}
            onClick={(e) => item.action(e.currentTarget)}
          >
            <IconSlot width={20}>
              {item.image ? <img src={item.image} alt="" width={14} height={14} /> : <Icon name={item.icon} size={14} />}
            </IconSlot>
            <Text size="callout" truncate grow>{item.verbatim ? item.title : tr(item.title)}</Text>
            {item.detail ? <Text size="body" tone="secondary">{item.detail}</Text> : null}
            {item.chevron ? <Icon name="chevron.right" size={11} color="var(--secondary)" /> : null}
          </button>
          {after?.(i)}
        </div>
      ))}
    </VStack>
  )
}
