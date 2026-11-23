import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Spinner } from '../components/Controls'
import { Icon } from '../components/Icon'
import { IconSlot, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { useAccount } from '../store/account'
import { useTr } from '../lib/use-language'


// The account's avatar (on the rail, where it opens Settings) and the keyboard-driven rows of the roster's
// right-click actions.

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
  chevron?: boolean
  disabled?: boolean
  destructive?: boolean
  /** A name the user gave: shown as typed, never translated. */
  verbatim?: boolean
  action: (anchor: HTMLButtonElement | null) => void
  submenu?: boolean
  expanded?: boolean
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
              <Icon name={item.icon} size={14} />
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
