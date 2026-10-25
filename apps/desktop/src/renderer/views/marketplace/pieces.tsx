import { useEffect, useEffectEvent, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import { IconButton } from '../../components/Controls'
import { Sheet, usePresence } from '../../components/Overlay'
import { Icon } from '../../components/Icon'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { EXTENSION_SECTIONS, type ExtensionSection } from './market-models'
import { ThinkingOrb } from '../../components/ThinkingOrb'
import './marketplace.css'
import './extensions-layout.css'

// Building blocks the Marketplace's screens share (kit MarketplaceView.swift "Pieces").

export function MarketSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="market-section">
      <Text as="div" size="headline" tone="text" className="market-section-title">{title}</Text>
      {children}
    </section>
  )
}

/** Two columns on wide screens, one on a narrow one. */
export function ItemGrid({ children }: { children: ReactNode }) {
  return <div className="item-grid">{children}</div>
}

export function MarketRow({ title, subtitle, added, busy = false, addLabel = 'Add', icon, onAdd, onOpen }: {
  title: string
  subtitle: string
  added: boolean
  busy?: boolean
  addLabel?: string
  icon: ReactNode
  onAdd: () => void
  /** The whole row opens the item's details; without it the row does what its button does. */
  onOpen?: () => void
}) {
  const tr = useTr()
  const addedLabel = tr('Added')
  return (
    <ItemRow
      title={title}
      subtitle={subtitle}
      icon={icon}
      onOpen={onOpen ?? (added || busy ? undefined : onAdd)}
      trailing={busy ? (
        <HStack as="span" align="stretch" justify="center" className="market-row-status">
          <ThinkingOrb size={18} color="var(--secondary)" />
        </HStack>
      ) : added ? (
        <HStack as="span" align="stretch" justify="center" className="market-row-status market-row-added" aria-label={addedLabel} title={addedLabel}>
          <Icon name="checkmark" size={11} weight="semibold" />
        </HStack>
      ) : (
        <button
          className="capsule-action"
          onClick={(e) => {
            e.stopPropagation()
            onAdd()
          }}
        >
          {tr(addLabel)}
        </button>
      )}
    />
  )
}

/** An `ItemGrid` row: icon, title and subtitle, then `trailing`; `onOpen` makes the whole row a button. */
export function ItemRow({ title, subtitle, icon, trailing, onOpen }: {
  title: string
  subtitle: string
  icon: ReactNode
  /** Buttons here stop the click so it doesn't also open the row. */
  trailing?: ReactNode
  onOpen?: () => void
}) {
  return (
    <div
      className={`market-row ${onOpen ? 'openable' : ''}`}
      role={onOpen ? 'button' : undefined}
      tabIndex={onOpen ? 0 : undefined}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (onOpen && e.target === e.currentTarget && (e.key === 'Enter' || e.key === ' ')) {
          e.preventDefault()
          onOpen()
        }
      }}
    >
      <div className="market-row-icon">{icon}</div>
      <VStack gap={2} grow>
        <Text size="body" weight="medium" tone="text" truncate title={title}>{title}</Text>
        <Text size="subheadline" tone="secondary" truncate title={subtitle}>{subtitle}</Text>
      </VStack>
      {trailing}
    </div>
  )
}

/** Placeholder rows while a list loads, shaped like the real ones. */
export function SkeletonGrid() {
  const tr = useTr()
  return (
    <VStack gap={8} aria-busy="true" aria-label={tr('Loading…')}>
      <Text size="footnote" tone="secondary">{tr('Loading…')}</Text>
      <div className="item-grid skeleton">
        {[0, 1, 2, 3].map((i) => (
          <HStack key={i} gap={14} className="skeleton-row">
            <div className="skeleton-icon" />
            <VStack gap={8}>
              <div className="skeleton-title" />
              <div className="skeleton-subtitle" />
            </VStack>
          </HStack>
        ))}
      </div>
    </VStack>
  )
}

/**
 * The end of a list the registry continues (kit `LoadMoreRow`). In view, it asks for the page at
 * `cursor`, and again each time the cursor moves on while it stays in view. After a failure it shows
 * Try again, and asks again once it scrolls back into view.
 */
export function LoadMoreRow({ cursor, failed, onLoad }: { cursor: string; failed: boolean; onLoad: () => void }) {
  const end = useRef<HTMLDivElement>(null)
  const [inView, setInView] = useState(false)
  const load = useEffectEvent(onLoad)
  useEffect(() => {
    const seen = new IntersectionObserver(([e]) => setInView(e!.isIntersecting))
    seen.observe(end.current!)
    return () => seen.disconnect()
  }, [])
  useEffect(() => {
    if (inView) load()
  }, [inView, cursor])
  return (
    <div ref={end} className="load-more">
      {failed ? (
        <HStack justify="center">
          <IconButton title="Try again" icon="arrow.clockwise" onClick={onLoad} />
        </HStack>
      ) : <SkeletonGrid />}
    </div>
  )
}

/** A loaded image, or nothing until (and unless) it loads. */
function RemoteImage({ src, className, style }: { src: string; className: string; style: CSSProperties }) {
  const [loaded, setLoaded] = useState(false)
  return <img src={src} alt="" draggable={false} onLoad={() => setLoaded(true)} className={`${className}${loaded ? ' loaded' : ''}`} style={style} />
}

function serviceDomain(website: string | null | undefined, name: string, registryName: string | null | undefined) {
  if (website && URL.canParse(website)) {
    const host = new URL(website).hostname
    if (host && host !== 'github.com') return host
  }
  // MCP Registry names are reverse-DNS ("com.notion/mcp"): the owner's domain.
  const owner = registryName?.split('/')[0]
  if (!owner) return null
  const labels = owner.split('.').filter(Boolean)
  // io.github.<user> is a GitHub account, not the service's own site.
  if (labels.length < 2 || (labels[0] === 'io' && labels[1] === 'github')) return name.toLowerCase().includes('github') ? 'github.com' : null
  return `${labels[1]}.${labels[0]}`
}

/** A service's logo from its website favicon, on a white tile; its initial otherwise. `site`: the
 *  service's own domain when it's known (a login's site), even github.com. */
export function ServiceLogo({ website, site, name, registryName, size = 46 }: { website?: string | null; site?: string | null; name: string; registryName?: string | null; size?: number }) {
  const domain = site ?? serviceDomain(website, name, registryName)
  const radius = size * 0.26
  return (
    <div className="market-logo service-logo" style={{ width: size, height: size, borderRadius: radius }}>
      <Text size={size * 0.4} weight="semibold" tone="text" className="service-logo-initial">{name.slice(0, 1).toUpperCase() || '?'}</Text>
      {domain ? (
        <RemoteImage
          src={`https://www.google.com/s2/favicons?domain=${encodeURIComponent(domain)}&sz=128`}
          className="market-logo-image service-logo-image"
          style={{ padding: size * 0.2 }}
        />
      ) : null}
    </div>
  )
}

/** An app's logo from Composio, a letter tile until it loads. */
export function AppLogo({ url, name, size = 46 }: { url?: string | null; name: string; size?: number }) {
  const [loaded, setLoaded] = useState(false)
  return (
    <div aria-hidden className="market-logo" style={{ width: size, height: size, borderRadius: size * 0.28 }}>
      {!loaded ? <Text size={size * 0.4} weight="semibold" tone="secondary">{name.slice(0, 1).toUpperCase()}</Text> : null}
      {url ? (
        <img
          src={url}
          alt=""
          draggable={false}
          onLoad={() => setLoaded(true)}
          className={`market-logo-image${loaded ? ' loaded' : ''}`}
          style={{ padding: size * 0.18 }}
        />
      ) : null}
    </div>
  )
}

export function TileIcon({ name }: { name: string }) {
  return (
    <div className="market-tile">
      <Icon name={name} size={18} weight="medium" />
    </div>
  )
}

export function SkillGlyph() {
  return (
    <div className="market-tile">
      <Icon name="book.pages" size={13} />
    </div>
  )
}

/** Text that opens a web page (replaces the system `Link`). */
export function WebLink({ title, url, className }: { title: string; url: string; className?: string }) {
  const tr = useTr()
  return (
    <button className={`plain-link${className ? ` ${className}` : ''}`} title={url} onClick={() => window.codync.app.openExternal(url)}>
      {tr(title)}
    </button>
  )
}

/** A full screen's top bar (kit `ScreenHeader`): back on the leading side, centered title, trailing. */
export function ScreenHeader({ title, onBack, trailing }: { title: string; onBack: () => void; trailing?: ReactNode }) {
  const tr = useTr()
  return (
    <HStack gap={6} className="market-screen-header">
      <Text as="div" size="compactBody" weight="semibold" tone="text" truncate className="market-screen-title">{tr(title)}</Text>
      <IconButton title="Back" icon="chevron.left" onClick={onBack} />
      <Spacer min={8} />
      {trailing}
    </HStack>
  )
}

/**
 * A screen pushed over another inside one modal: the base slides out leading as `top` slides
 * in from the trailing edge (kit's `.move(edge:)` + opacity transitions), and back.
 */
export function SlideStack({ base, top }: { base: ReactNode; top: ReactNode | null }) {
  const { mounted, shown } = usePresence(top !== null)
  const last = useRef(top)
  if (top !== null) last.current = top
  return (
    <div className="market-stack">
      <div className={`market-layer ${shown ? 'off-leading' : ''}`} inert={shown}>{base}</div>
      {mounted ? <div className={`market-layer ${shown ? '' : 'off-trailing'}`}>{top ?? last.current}</div> : null}
    </div>
  )
}

/** Plain text entry for card rows: one line, a secret, or a growing multi-line box. */
export function Field({ value, onChange, placeholder, secret = false, lines, mono = false, label }: {
  value: string
  onChange: (v: string) => void
  placeholder?: string
  secret?: boolean
  /** Multi-line, growing between these line counts (`axis: .vertical` + `.lineLimit`). */
  lines?: [number, number]
  mono?: boolean
  label?: string
}) {
  const tr = useTr()
  const hint = placeholder ? tr(placeholder) : undefined
  const named = label ? tr(label) : hint
  const className = `market-field${mono ? ' mono' : ''}`
  if (lines) {
    return (
      <textarea
        className={className}
        aria-label={named}
        value={value}
        placeholder={hint}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value)}
        style={{ minHeight: `${lines[0] * 1.3}em`, maxHeight: `${lines[1] * 1.3}em` }}
      />
    )
  }
  return (
    <input
      className={className}
      aria-label={named}
      type={secret ? 'password' : 'text'}
      value={value}
      placeholder={hint}
      spellCheck={false}
      autoComplete="off"
      onChange={(e) => onChange(e.target.value)}
    />
  )
}

/** A danger-colored error line in its own card. */
export function ErrorText({ text }: { text: string }) {
  return <Text tone="danger" className="selectable">{text}</Text>
}

/** Values with something typed in. */
export const filled = (values: Record<string, string>) => Object.fromEntries(Object.entries(values).filter(([, v]) => v !== ''))

/** The window's height, following resizes. */
export function useWindowHeight() {
  const [height, setHeight] = useState(window.innerHeight)
  useEffect(() => {
    const onResize = () => setHeight(window.innerHeight)
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])
  return height
}

/** Agents, Connectors, Skills: a row of filled chips over the list, the chosen one in the text color. */
export function ExtensionTabs({ selection, onChange }: { selection: ExtensionSection; onChange: (id: ExtensionSection) => void }) {
  const tr = useTr()
  return (
    <nav className="market-tabs" role="tablist" aria-label={tr('Extensions sections')}>
      {EXTENSION_SECTIONS.map((section) => {
        const on = section.id === selection
        return (
          <button key={section.id} type="button" role="tab" aria-selected={on} className={`market-tab${on ? ' on' : ''}`} onClick={() => onChange(section.id)}>
            {tr(section.label)}
          </button>
        )
      })}
    </nav>
  )
}

/**
 * A Marketplace form in its own modal card (`.codyncSheet`), as tall as its content up to
 * `height`; `fixed` keeps it at `height` for content that fills the card (a terminal).
 */
export function FormSheet({ open, onClose, width = 520, height = 600, fixed = false, children }: { open: boolean; onClose: () => void; width?: number; height?: number; fixed?: boolean; children: ReactNode }) {
  const windowHeight = useWindowHeight()
  const cap = Math.min(height, windowHeight - 76)
  return (
    <Sheet open={open} onClose={onClose} width={width} height={fixed ? cap : undefined} maxHeight={cap}>
      {children}
    </Sheet>
  )
}
