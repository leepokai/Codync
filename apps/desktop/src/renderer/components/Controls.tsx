import { Children, Fragment, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import { font } from '../lib/fonts'
import { Icon } from './Icon'
import { AnchoredMenu, IconButtonBase, type MenuItem } from './Overlay'
import './controls.css'

// Codync's own controls (kit Controls.swift): never the stock ones.

export const IconButton = IconButtonBase

/** The filled call-to-action (`.primary`) or the quieter filled one (`.secondary`). */
export function Button({ kind = 'primary', children, onClick, disabled, style, className, title }: {
  kind?: 'primary' | 'secondary'
  children: ReactNode
  onClick?: () => void
  disabled?: boolean
  style?: CSSProperties
  className?: string
  title?: string
}) {
  return (
    <button className={`styled-button ${kind} ${className ?? ''}`} onClick={onClick} disabled={disabled} style={style} title={title}>
      {children}
    </button>
  )
}

/** Loading indicator: a thin arc that turns. */
export function Spinner({ size = 14 }: { size?: number }) {
  const w = Math.max(1.5, size / 9)
  const r = (size - w) / 2
  const c = 2 * Math.PI * r
  return (
    <svg className="spinner" width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-label="Loading" role="img">
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--secondary)" strokeWidth={w} strokeLinecap="round" strokeDasharray={`${c * 0.7} ${c}`} />
    </svg>
  )
}

/** On/off switch: label on the left, a flat capsule track on the right. */
export function Switch({ on, onChange, children, disabled, style }: { on: boolean; onChange: (on: boolean) => void; children?: ReactNode; disabled?: boolean; style?: CSSProperties }) {
  return (
    <button className="switch" role="switch" aria-checked={on} disabled={disabled} onClick={() => onChange(!on)} style={style}>
      {children !== undefined ? <span className="switch-label">{children}</span> : null}
      <span className={`switch-track ${on ? 'on' : ''}`}>
        <span className="switch-knob" />
      </span>
    </button>
  )
}

/** A button that opens a Codync menu. */
export function DropdownMenu({ items, children, className, style, title, search }: { items: () => MenuItem[]; children: ReactNode; className?: string; style?: CSSProperties; title?: string; search?: { value: string; onChange: (v: string) => void; placeholder?: string } }) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLButtonElement>(null)
  return (
    <>
      <button ref={ref} className={className} style={style} title={title} onClick={() => setOpen((o) => !o)}>
        {children}
      </button>
      <AnchoredMenu open={open} onClose={() => setOpen(false)} anchor={ref} items={items} search={search} />
    </>
  )
}

/** Picks one value: the current choice in a pill with a chevron, opening a Codync menu. */
export function ChoicePicker<T extends string | number>({ selection, options, onChange, fill, fits = false, search }: {
  selection: T
  options: { id: T; label: string }[]
  onChange: (id: T) => void
  fill?: string
  fits?: boolean
  search?: { value: string; onChange: (v: string) => void; placeholder?: string }
}) {
  const label = options.find((o) => o.id === selection)?.label ?? 'Choose'
  return (
    <DropdownMenu
      title={label}
      className={`choice-picker ${fill ? '' : 'outlined'}`}
      style={{ background: fill, width: fits ? '100%' : undefined }}
      items={() => options.map((o) => ({ title: o.label, selected: o.id === selection, action: () => onChange(o.id) }))}
      search={search}
    >
      <span className="choice-label">{label}</span>
      <Icon name="chevron.down" size={10} weight="semibold" color="var(--secondary)" />
    </DropdownMenu>
  )
}

/** Picks one of a few values side by side: the replacement for `.segmented`. */
export function SegmentedChoice<T extends string>({ selection, options, onChange }: { selection: T; options: { id: T; label: string }[]; onChange: (id: T) => void }) {
  const index = Math.max(0, options.findIndex((o) => o.id === selection))
  return (
    <div className="segmented">
      <div className="segmented-thumb" style={{ width: `calc((100% - 6px - ${(options.length - 1) * 2}px) / ${options.length})`, transform: `translateX(calc(${index} * (100% + 2px)))` }} />
      {options.map((o) => (
        <button key={o.id} className={`segmented-option ${o.id === selection ? 'on' : ''}`} onClick={() => onChange(o.id)} aria-pressed={o.id === selection}>
          {o.label}
        </button>
      ))}
    </div>
  )
}

/** A list of choices, one per row with a checkmark: the replacement for `.inline` pickers. */
export function ChoiceList<T extends string>({ selection, options, onChange }: { selection: T; options: { id: T; label: string; detail?: string | null }[]; onChange: (id: T) => void }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      {options.map((o) => (
        <button key={o.id} className="choice-row" onClick={() => onChange(o.id)}>
          <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 2 }}>
            <span style={{ color: 'var(--text)' }}>{o.label}</span>
            {o.detail ? <span style={{ ...font('compactSecondary'), color: 'var(--secondary)' }}>{o.detail}</span> : null}
          </div>
          <span style={{ opacity: o.id === selection ? 1 : 0, display: 'flex' }}>
            <Icon name="checkmark" size={10} weight="semibold" />
          </span>
        </button>
      ))}
    </div>
  )
}

/** A scrolling page of cards: the replacement for `Form` / grouped `List`. */
export function CardForm({ children, style }: { children: ReactNode; style?: CSSProperties }) {
  return (
    <div className="card-form" style={style}>
      <div className="card-form-content">{children}</div>
    </div>
  )
}

/**
 * A titled group of rows (ChatGPT's desktop settings): a small bold heading over a filled,
 * rounded group whose rows are split by inset hairlines.
 */
export function CardSection({ title, footer, accessory, children }: { title?: string | null; footer?: string | null; accessory?: ReactNode; children: ReactNode }) {
  const rows = Children.toArray(children)
  return (
    <section className="card-section">
      {title ? (
        <div className="card-section-title">
          <span style={{ ...font(13, 'semibold'), color: 'var(--text)' }}>{title}</span>
          <span style={{ flex: 1 }} />
          {accessory}
        </div>
      ) : null}
      <div className="card-section-rows">
        {rows.map((row, i) => (
          <Fragment key={i}>
            <div className="card-row">{row}</div>
            {i < rows.length - 1 ? <div className="hairline inset" /> : null}
          </Fragment>
        ))}
      </div>
      {footer ? <div className="card-section-footer">{footer}</div> : null}
    </section>
  )
}

/** A 1-pixel separator line in the border color. */
export function Hairline({ style }: { style?: CSSProperties }) {
  return <div className="hairline" style={style} />
}

/** Label on the left, value on the right: the replacement for `LabeledContent`. */
export function ValueRow({ label, detail, children }: { label: string; detail?: string | null; children?: ReactNode }) {
  return (
    <div className="value-row">
      <div style={{ display: 'flex', flexDirection: 'column', gap: 3, minWidth: 0 }}>
        <span style={{ color: 'var(--text)' }}>{label}</span>
        {detail ? <span style={{ ...font('caption'), color: 'var(--secondary)', lineHeight: 1.35 }}>{detail}</span> : null}
      </div>
      <span style={{ flex: 1, minWidth: 12 }} />
      <span style={{ color: 'var(--secondary)', display: 'flex', alignItems: 'center' }}>{children}</span>
    </div>
  )
}

/** A search box: the replacement for `.searchable`. */
export function SearchField({ value, onChange, prompt = 'Search', inputRef }: { value: string; onChange: (v: string) => void; prompt?: string; inputRef?: React.RefObject<HTMLInputElement | null> }) {
  return (
    <div className="search-field">
      <Icon name="magnifyingglass" size={12} color="var(--secondary)" />
      <input ref={inputRef} value={value} placeholder={prompt} spellCheck={false} onChange={(e) => onChange(e.target.value)} />
      {value ? (
        <button aria-label="Clear search" onClick={() => onChange('')} style={{ display: 'flex', color: 'var(--tertiary)' }}>
          <Icon name="xmark.circle.fill" size={12} />
        </button>
      ) : null}
    </div>
  )
}

/** A row that expands to show more: the replacement for `DisclosureGroup`. */
export function Disclosure({ expanded, onToggle, label, children }: { expanded: boolean; onToggle: () => void; label: ReactNode; children: ReactNode }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      <button className="disclosure-label" onClick={onToggle} aria-expanded={expanded}>
        {label}
        <span style={{ flex: 1, minWidth: 8 }} />
        <span style={{ display: 'flex', transform: `rotate(${expanded ? 90 : 0}deg)`, transition: 'transform var(--layout)', color: 'var(--secondary)' }}>
          <Icon name="chevron.right" size={10} weight="semibold" />
        </span>
      </button>
      {expanded ? children : null}
    </div>
  )
}

/** A filled rounded value box, or outlined (outline or fill, never both). */
export function Pill({ children, fill = 'var(--background)', outlined = false, style }: { children: ReactNode; fill?: string; outlined?: boolean; style?: CSSProperties }) {
  return (
    <span className="pill" style={{ background: outlined ? 'transparent' : fill, boxShadow: outlined ? 'inset 0 0 0 1px var(--border)' : undefined, ...style }}>
      {children}
    </span>
  )
}

/** Determinate progress (`value` 0...1): a thin capsule track that fills from the left. */
export function ProgressBar({ value, label }: { value: number; label: string }) {
  return (
    <div className="progress-track" role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(value * 100)}>
      <div className="progress-fill" style={{ transform: `scaleX(${value})` }} />
    </div>
  )
}
