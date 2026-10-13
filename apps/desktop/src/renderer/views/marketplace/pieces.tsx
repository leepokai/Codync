import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import { IconButton } from '../../components/Controls'
import { Sheet, usePresence } from '../../components/Overlay'
import { Icon } from '../../components/Icon'
import { ThinkingOrb } from '../../components/ThinkingOrb'
import { font } from '../../lib/fonts'
import './marketplace.css'

// Building blocks the Marketplace's screens share (kit MarketplaceView.swift "Pieces").

export function MarketSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="market-section">
      <div style={{ ...font('headline'), color: 'var(--text)', paddingLeft: 4 }}>{title}</div>
      {children}
    </section>
  )
}

/** Two columns on wide screens, one on a narrow one. */
export function ItemGrid({ children }: { children: ReactNode }) {
  return <div className="item-grid">{children}</div>
}

const oneLine: CSSProperties = { whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }
export const lineClamp = (n: number): CSSProperties => ({ display: '-webkit-box', WebkitLineClamp: n, WebkitBoxOrient: 'vertical', overflow: 'hidden' })

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
  const open = onOpen ?? (added || busy ? undefined : onAdd)
  return (
    <div
      className={`market-row ${open ? 'openable' : ''}`}
      role={open ? 'button' : undefined}
      tabIndex={open ? 0 : undefined}
      onClick={open}
      onKeyDown={(e) => {
        if (open && e.target === e.currentTarget && (e.key === 'Enter' || e.key === ' ')) {
          e.preventDefault()
          open()
        }
      }}
    >
      <div style={{ width: 46, height: 46, flex: 'none', display: 'grid' }}>{icon}</div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 2, minWidth: 0, flex: 1 }}>
        <span style={{ ...font('body', 'medium'), color: 'var(--text)', ...oneLine }} title={title}>{title}</span>
        <span style={{ ...font('subheadline'), color: 'var(--secondary)', ...oneLine }} title={subtitle}>{subtitle}</span>
      </div>
      {busy ? (
        <span style={{ width: 60, display: 'flex', justifyContent: 'center', flex: 'none' }}>
          <ThinkingOrb size={18} color="var(--secondary)" />
        </span>
      ) : added ? (
        <span style={{ width: 60, display: 'flex', justifyContent: 'center', flex: 'none', color: 'var(--secondary)' }} aria-label="Added" title="Added">
          <Icon name="checkmark" size={11} weight="semibold" />
        </span>
      ) : (
        <button
          className="capsule-action"
          style={font('subheadline', 'semibold')}
          onClick={(e) => {
            e.stopPropagation()
            onAdd()
          }}
        >
          {addLabel}
        </button>
      )}
    </div>
  )
}

/** Placeholder rows while a list loads, shaped like the real ones. */
export function SkeletonGrid() {
  return (
    <div className="item-grid skeleton">
      {[0, 1, 2, 3].map((i) => (
        <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 14, padding: '10px 12px' }}>
          <div style={{ width: 46, height: 46, borderRadius: 12, background: 'var(--bubble-agent)', flex: 'none' }} />
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            <div style={{ width: 120, height: 12, borderRadius: 6, background: 'var(--bubble-agent)' }} />
            <div style={{ width: 190, height: 10, borderRadius: 5, background: 'var(--bubble-agent)' }} />
          </div>
        </div>
      ))}
    </div>
  )
}

/** A loaded image, or nothing until (and unless) it loads. */
function RemoteImage({ src, style }: { src: string; style: CSSProperties }) {
  const [loaded, setLoaded] = useState(false)
  return <img src={src} alt="" draggable={false} onLoad={() => setLoaded(true)} style={{ ...style, display: loaded ? 'block' : 'none' }} />
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

/** A service's logo from its website favicon, on a white tile; its initial otherwise. */
export function ServiceLogo({ website, name, registryName, size = 46 }: { website?: string | null; name: string; registryName?: string | null; size?: number }) {
  const domain = serviceDomain(website, name, registryName)
  const radius = size * 0.26
  return (
    <div style={{ position: 'relative', width: size, height: size, borderRadius: radius, overflow: 'hidden', background: 'var(--bubble-agent)', display: 'grid', placeItems: 'center', flex: 'none' }}>
      <span style={{ ...font(size * 0.4, 'semibold', 'rounded'), color: 'var(--text)' }}>{name.slice(0, 1).toUpperCase() || '?'}</span>
      {domain ? (
        <RemoteImage
          src={`https://www.google.com/s2/favicons?domain=${encodeURIComponent(domain)}&sz=128`}
          style={{ position: 'absolute', inset: 0, width: '100%', height: '100%', objectFit: 'contain', padding: size * 0.2, boxSizing: 'border-box', background: '#fff' }}
        />
      ) : null}
    </div>
  )
}

/** An app's logo from Composio, a letter tile until it loads. */
export function AppLogo({ url, name, size = 46 }: { url?: string | null; name: string; size?: number }) {
  const [loaded, setLoaded] = useState(false)
  return (
    <div aria-hidden style={{ position: 'relative', width: size, height: size, flex: 'none', borderRadius: size * 0.28, background: 'var(--bubble-agent)', display: 'grid', placeItems: 'center' }}>
      {!loaded ? <span style={{ ...font(size * 0.4, 'semibold'), color: 'var(--secondary)' }}>{name.slice(0, 1).toUpperCase()}</span> : null}
      {url ? (
        <img
          src={url}
          alt=""
          draggable={false}
          onLoad={() => setLoaded(true)}
          style={{ position: 'absolute', inset: 0, width: '100%', height: '100%', objectFit: 'contain', padding: size * 0.18, boxSizing: 'border-box', display: loaded ? 'block' : 'none' }}
        />
      ) : null}
    </div>
  )
}

export function TileIcon({ name }: { name: string }) {
  return (
    <div style={{ width: '100%', height: '100%', borderRadius: 12, background: 'var(--bubble-agent)', display: 'grid', placeItems: 'center', color: 'var(--text)' }}>
      <Icon name={name} size={18} weight="medium" />
    </div>
  )
}

export function SkillGlyph({ size }: { size?: number }) {
  return (
    <div style={{ width: size ?? '100%', height: size ?? '100%', flex: 'none', borderRadius: 12, background: 'var(--bubble-agent)', display: 'grid', placeItems: 'center', color: 'var(--text)' }}>
      <Icon name="book.pages" size={13} />
    </div>
  )
}

/** Text that opens a web page (replaces the system `Link`). */
export function WebLink({ title, url, style }: { title: string; url: string; style?: CSSProperties }) {
  return (
    <button className="plain-link" style={style} title={url} onClick={() => window.codync.app.openExternal(url)}>
      {title}
    </button>
  )
}

/** A full screen's top bar (kit `ScreenHeader`): back on the leading side, centered title, trailing. */
export function ScreenHeader({ title, onBack, trailing }: { title: string; onBack: () => void; trailing?: ReactNode }) {
  return (
    <div style={{ position: 'relative', height: 44, padding: '0 12px', display: 'flex', alignItems: 'center', gap: 6, flex: 'none', background: 'var(--background)' }}>
      <div style={{ position: 'absolute', left: 60, right: 60, textAlign: 'center', ...font('compactBody', 'semibold'), color: 'var(--text)', ...oneLine }}>{title}</div>
      <IconButton title="Back" icon="chevron.left" onClick={onBack} />
      <span style={{ flex: 1, minWidth: 8 }} />
      {trailing}
    </div>
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
  const style: CSSProperties = mono ? font('callout', 'regular', 'monospaced') : {}
  if (lines) {
    return (
      <textarea
        className="market-field"
        aria-label={label ?? placeholder}
        value={value}
        placeholder={placeholder}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value)}
        style={{ ...style, lineHeight: 1.3, minHeight: `${lines[0] * 1.3}em`, maxHeight: `${lines[1] * 1.3}em` }}
      />
    )
  }
  return (
    <input
      className="market-field"
      aria-label={label ?? placeholder}
      type={secret ? 'password' : 'text'}
      value={value}
      placeholder={placeholder}
      spellCheck={false}
      autoComplete="off"
      onChange={(e) => onChange(e.target.value)}
      style={style}
    />
  )
}

/** A danger-colored error line in its own card. */
export function ErrorText({ text }: { text: string }) {
  return <span className="selectable" style={{ color: 'var(--danger)' }}>{text}</span>
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

/** A Marketplace form in its own modal card (`.codyncSheet`). */
export function FormSheet({ open, onClose, width = 520, height = 600, children }: { open: boolean; onClose: () => void; width?: number; height?: number; children: ReactNode }) {
  const windowHeight = useWindowHeight()
  return (
    <Sheet open={open} onClose={onClose} width={width} height={Math.min(height, windowHeight - 76)}>
      {children}
    </Sheet>
  )
}
