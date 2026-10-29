import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { Spinner } from '../components/Controls'
import { Icon } from '../components/Icon'
import type { MenuItem } from '../components/Overlay'
import { HStack, IconSlot, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import type { Link } from '@shared/analytics'
import { account, useAccount } from '../store/account'
import { openLink, useScreenViewed } from '../store/analytics'
import googleLogo from '../assets/google.svg'
import { useTr } from '../lib/use-language'


// The chat window's floating panels: the account menu (with its profile avatar) and the roster's
// right-click actions, both built on the same keyboard-driven rows.

export function ProfileAvatar() {
  const { user, isBusy } = useAccount()
  return (
    <span className="profile-avatar" aria-hidden>
      {user?.avatarURL ? (
        <img src={user.avatarURL} alt="" className="profile-avatar-image" />
      ) : (
        <Icon name="person.fill" size={15} weight="medium" color="var(--secondary)" />
      )}
      {isBusy ? (
        <HStack as="span" justify="center" className="profile-avatar-busy">
          <Spinner size={14} />
        </HStack>
      ) : null}
    </span>
  )
}

interface PanelItem {
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
  action: () => void
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

function PanelRows({ items, onDismiss, after }: { items: PanelItem[]; onDismiss: () => void; after?: (i: number) => ReactNode }) {
  const tr = useTr()
  const [highlighted, setHighlighted] = useState(0)
  const highlightedRow = useRef<HTMLButtonElement>(null)
  // A long list scrolls: ↑/↓ and Tab bring the row Enter would run into view.
  // A block, not an arrow's value: Chromium's scrollIntoView returns a promise, which React would call as the clean-up.
  useEffect(() => {
    highlightedRow.current?.scrollIntoView({ block: 'nearest' })
  }, [highlighted])
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        onDismiss()
      } else if (e.key === 'ArrowDown' || (e.key === 'Tab' && !e.shiftKey)) {
        e.preventDefault()
        setHighlighted((h) => (h + 1) % items.length)
      } else if (e.key === 'ArrowUp' || (e.key === 'Tab' && e.shiftKey)) {
        e.preventDefault()
        setHighlighted((h) => (h - 1 + items.length) % items.length)
      } else if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault()
        const item = items[highlighted]
        if (item && !item.disabled) item.action()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [items, highlighted, onDismiss])
  return (
    <VStack gap={2}>
      {items.map((item, i) => (
        <div key={item.title}>
          <button
            ref={highlighted === i ? highlightedRow : undefined}
            className={`panel-row ${highlighted === i ? 'highlight' : ''} ${item.destructive ? 'destructive' : ''}`}
            disabled={item.disabled}
            // Movement, not enter: Chromium re-enters the row under a resting pointer after a scroll, which would undo ↑/↓.
            onMouseMove={() => setHighlighted(i)}
            onClick={item.action}
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

/** A roster menu row: an action, or with `submenu`, rows that open in the menu's place (as the account menu opens Support). */
export type ActionItem = MenuItem | (Omit<MenuItem, 'action'> & { submenu: MenuItem[] })

/** The roster's right-click actions at `x`, `y`, shared by pointer and keyboard; kept inside the window, a long list scrolling. */
export function DesktopActionMenu({ items, onDismiss, x, y }: { items: ActionItem[]; onDismiss: () => void; x: number; y: number }) {
  const [submenu, setSubmenu] = useState<{ title: string; items: MenuItem[] } | null>(null)
  const menu = useRef<HTMLDivElement>(null)
  // Measured after every render: a submenu, or a window resized under it, changes its height.
  const [height, setHeight] = useState(0)
  useLayoutEffect(() => setHeight(menu.current?.offsetHeight ?? 0))
  const row = (m: ActionItem): PanelItem => 'submenu' in m
    ? { title: m.title, icon: m.icon ?? 'circle', chevron: true, action: () => setSubmenu({ title: m.title, items: m.submenu }) }
    : { title: m.title, icon: m.icon ?? 'circle', destructive: m.destructive, verbatim: m.verbatim, action: () => { onDismiss(); m.action() } }
  const rows: { row: PanelItem; divider?: boolean }[] = submenu
    ? [{ row: { title: submenu.title, icon: 'chevron.left', action: () => setSubmenu(null) } }, ...submenu.items.map((m, i) => ({ row: row(m), divider: i === 0 || m.divider }))]
    : items.map((m) => ({ row: row(m), divider: m.divider }))
  return (
    <div ref={menu} className="action-menu" style={{ left: Math.min(Math.max(8, x), window.innerWidth - 268), top: Math.max(8, Math.min(y, window.innerHeight - 8 - height)) }}>
      <div className="action-menu-rows">
        <PanelRows
          key={submenu?.title}
          items={rows.map((r) => r.row)}
          onDismiss={onDismiss}
          after={(i) => (rows[i + 1]?.divider ? <div className="panel-divider" /> : null)}
        />
      </div>
    </div>
  )
}
