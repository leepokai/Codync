import { Children, Fragment, useEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent, type ReactNode } from 'react'
import { Icon } from './Icon'
import { AnchoredMenu, IconButtonBase, usePresence, type MenuItem } from './Overlay'
import { HStack, Spacer, VStack } from './Stack'
import { useTr } from '../lib/use-language'
import { Text } from './Text'
import './controls.css'

// Codync's own controls (kit Controls.swift): never the stock ones.

export const IconButton = IconButtonBase

/** The filled call-to-action (`.primary`) or the quieter filled one (`.secondary`). */
export function Button({ kind = 'primary', children, onClick, disabled, className, title }: {
  kind?: 'primary' | 'secondary'
  children: ReactNode
  onClick?: () => void
  disabled?: boolean
  className?: string
  title?: string
}) {
  const tr = useTr()
  return (
    <button className={`styled-button ${kind} ${className ?? ''}`} onClick={onClick} disabled={disabled} title={title ? tr(title) : title}>
      {typeof children === 'string' ? tr(children) : children}
    </button>
  )
}

/** Loading indicator: a thin arc that turns. */
export function Spinner({ size = 14 }: { size?: number }) {
  const { w, r, c, center } = ring(size)
  return (
    <svg className="spinner" width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-label={useTr()('Loading')} role="img">
      <circle cx={center} cy={center} r={r} fill="none" stroke="var(--secondary)" strokeWidth={w} strokeLinecap="round" strokeDasharray={`${c * 0.7} ${c}`} />
    </svg>
  )
}

/** Determinate progress (`value` 0...1): a ring that fills clockwise, in the color around it. */
export function ProgressRing({ value, size = 14 }: { value: number; size?: number }) {
  const { w, r, c, center } = ring(size)
  return (
    <svg className="progress-ring" width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden>
      <circle className="progress-ring-track" cx={center} cy={center} r={r} strokeWidth={w} />
      <circle
        className="progress-ring-fill"
        cx={center}
        cy={center}
        r={r}
        strokeWidth={w}
        strokeDasharray={c}
        strokeDashoffset={c * (1 - value)}
        transform={`rotate(-90 ${center} ${center})`}
      />
    </svg>
  )
}

/** The geometry `Spinner` and `ProgressRing` share: stroke width, radius, circumference. */
function ring(size: number) {
  const w = Math.max(1.5, size / 9)
  const r = (size - w) / 2
  return { w, r, c: 2 * Math.PI * r, center: size / 2 }
}

/** On/off switch: label on the left, a flat capsule track on the right. */
/** `label` names a switch whose visible label sits outside it (beside an info button, say). */
export function Switch({ on, onChange, children, label, disabled, className }: { on: boolean; onChange: (on: boolean) => void; children?: ReactNode; label?: string; disabled?: boolean; className?: string }) {
  const tr = useTr()
  return (
    <button className={`switch${className ? ` ${className}` : ''}`} role="switch" aria-checked={on} aria-label={label ? tr(label) : label} disabled={disabled} onClick={() => onChange(!on)}>
      {children !== undefined ? <span className="switch-label">{typeof children === 'string' ? tr(children) : children}</span> : null}
      <span className={`switch-track ${on ? 'on' : ''}`}>
        <span className="switch-knob" />
      </span>
    </button>
  )
}

/** A button that opens a Codync menu. */
export function DropdownMenu({ items, children, className, style, title }: { items: () => MenuItem[]; children: ReactNode; className?: string; style?: CSSProperties; title?: string }) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLButtonElement>(null)
  return (
    <>
      <button ref={ref} className={className} style={style} title={title} aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        {children}
      </button>
      <AnchoredMenu open={open} onClose={() => setOpen(false)} anchor={ref} items={items} />
    </>
  )
}

/** Picks one value: the current choice in a pill with a chevron, opening a Codync menu. */
export function ChoicePicker<T extends string | number>({ selection, options, onChange, fill, fits = false }: {
  selection: T
  /** `leading`: a picture before the label, in the menu and on the picker (an agent's logo). */
  options: { id: T; label: string; leading?: ReactNode }[]
  onChange: (id: T) => void
  fill?: string
  fits?: boolean
}) {
  const tr = useTr()
  const chosen = options.find((o) => o.id === selection)
  const label = tr(chosen?.label ?? 'Choose')
  return (
    <DropdownMenu
      title={label}
      className={`choice-picker${fill ? '' : ' outlined'}${fits ? ' fits' : ''}`}
      style={{ background: fill }}
      items={() => options.map((o) => ({ title: tr(o.label), leading: o.leading, selected: o.id === selection, action: () => onChange(o.id) }))}
    >
      {chosen?.leading}
      <span className="choice-label">{label}</span>
      <Icon name="chevron.down" size={10} weight="semibold" color="var(--secondary)" />
    </DropdownMenu>
  )
}

/** Picks one of a few values side by side: the replacement for `.segmented`. */
export function SegmentedChoice<T extends string>({ selection, options, onChange }: { selection: T; options: { id: T; label: string }[]; onChange: (id: T) => void }) {
  const tr = useTr()
  const index = options.findIndex((o) => o.id === selection)
  return (
    <div className="segmented">
      {index >= 0 && <div className="segmented-thumb" style={{ width: `calc((100% - 6px - ${(options.length - 1) * 2}px) / ${options.length})`, transform: `translateX(calc(${index} * (100% + 2px)))` }} />}
      {options.map((o) => (
        <button key={o.id} className={`segmented-option ${o.id === selection ? 'on' : ''}`} onClick={() => onChange(o.id)} aria-pressed={o.id === selection}>
          {tr(o.label)}
        </button>
      ))}
    </div>
  )
}

/** Grok Bot's page tabs: plain labels, the chosen one on a light capsule. */
export function TabChoice<T extends string>({ label, selection, options, onChange }: { label: string; selection: T; options: { id: T; label: string }[]; onChange: (id: T) => void }) {
  const tr = useTr()
  return (
    <div className="tab-choice" role="tablist" aria-label={tr(label)}>
      {options.map((o) => (
        <button key={o.id} role="tab" aria-selected={o.id === selection} className={`tab-choice-option ${o.id === selection ? 'on' : ''}`} onClick={() => onChange(o.id)}>
          {tr(o.label)}
        </button>
      ))}
    </div>
  )
}

/** Picks one of a few values as wrapping plain labels, the chosen one on a light capsule; `title` names a short label in full. */
export function ChipChoice<T extends string | number>({ label, selection, options, onChange }: {
  label: string
  selection: T
  options: readonly { id: T; label: string; title?: string }[]
  onChange: (id: T) => void
}) {
  const tr = useTr()
  return (
    <HStack gap={2} wrap role="radiogroup" aria-label={tr(label)}>
      {options.map((o) => (
        <button key={o.id} role="radio" aria-checked={o.id === selection} aria-label={o.title ? tr(o.title) : undefined} title={o.title ? tr(o.title) : undefined} className={`chip-choice${o.id === selection ? ' on' : ''}`} onClick={() => onChange(o.id)}>
          {tr(o.label)}
        </button>
      ))}
    </HStack>
  )
}

/** A list of choices, one per row with a checkmark: the replacement for `.inline` pickers. */
export function ChoiceList<T extends string>({ selection, options, onChange }: { selection: T; options: { id: T; label: string; detail?: string | null }[]; onChange: (id: T) => void }) {
  const tr = useTr()
  return (
    <VStack gap={10}>
      {options.map((o) => (
        <button key={o.id} className="choice-row" onClick={() => onChange(o.id)}>
          <VStack gap={2} className="choice-row-label">
            <Text tone="text">{tr(o.label)}</Text>
            {o.detail ? <Text size="compactSecondary" tone="secondary">{tr(o.detail)}</Text> : null}
          </VStack>
          <span className={`check-slot${o.id === selection ? ' on' : ''}`}>
            <Icon name="checkmark" size={10} weight="semibold" />
          </span>
        </button>
      ))}
    </VStack>
  )
}

/** A scrolling page of cards: the replacement for `Form` / grouped `List`. */
export function CardForm({ children }: { children: ReactNode }) {
  return (
    <div className="card-form">
      <div className="card-form-content">{children}</div>
    </div>
  )
}

/**
 * A titled group of rows (ChatGPT's desktop settings): a small bold heading over a filled,
 * rounded group whose rows are split by inset hairlines.
 */
export function CardSection({ title, footer, accessory, children }: { title?: ReactNode; footer?: ReactNode; accessory?: ReactNode; children: ReactNode }) {
  const tr = useTr()
  const rows = Children.toArray(children)
  const heading = typeof title === 'string' ? tr(title) : title
  const note = typeof footer === 'string' ? tr(footer) : footer
  return (
    <section className="card-section">
      {heading || accessory ? (
        <div className="card-section-title">
          {heading ? <Text as="h3" size="body" weight="semibold" tone="text">{heading}</Text> : null}
          <Spacer />
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
      {note ? <div className="card-section-footer">{typeof footer === 'string' ? <Text size="caption" tone="secondary">{note}</Text> : footer}</div> : null}
    </section>
  )
}

/** A 1-pixel separator line in the border color. */
export function Hairline({ className, style }: { className?: string; style?: CSSProperties }) {
  return <div className={`hairline${className ? ` ${className}` : ''}`} style={style} />
}

/** A row's title with an optional note under it (rows, fields and switches share it). */
export function RowLabel({ label, detail, labelId, detailId, grow = false }: { label: ReactNode; detail?: string | null; labelId?: string; detailId?: string; grow?: boolean }) {
  const tr = useTr()
  const heading = typeof label === 'string' ? tr(label) : label
  const note = detail ? tr(detail) : detail
  return (
    <VStack as="span" gap={3} grow={grow} shrink>
      <Text id={labelId} tone="text">{heading}</Text>
      {note ? <span id={detailId} className="detail-note" title={note}>{note}</span> : null}
    </VStack>
  )
}

/** Label on the left, value on the right: the replacement for `LabeledContent`. */
export function ValueRow({ label, detail, children }: { label: string; detail?: string | null; children?: ReactNode }) {
  return (
    <div className="value-row">
      <RowLabel label={label} detail={detail} />
      <Spacer min={12} />
      <HStack as="span" className="value-row-value">{children}</HStack>
    </div>
  )
}

/** A search box: the replacement for `.searchable`. */
export function SearchField({ value, onChange, prompt = 'Search', inputRef }: { value: string; onChange: (v: string) => void; prompt?: string; inputRef?: React.RefObject<HTMLInputElement | null> }) {
  const tr = useTr()
  const clear = tr('Clear search')
  return (
    <div className="search-field">
      <Icon name="magnifyingglass" size={12} color="var(--secondary)" />
      <input ref={inputRef} value={value} placeholder={tr(prompt)} spellCheck={false} onChange={(e) => onChange(e.target.value)} />
      {value ? (
        <button aria-label={clear} title={clear} className="search-clear" onClick={() => onChange('')}>
          <Icon name="xmark.circle.fill" size={12} />
        </button>
      ) : null}
    </div>
  )
}

/** A row that expands to show more: the replacement for `DisclosureGroup`. */
export function Disclosure({ expanded, onToggle, label, children }: { expanded: boolean; onToggle: () => void; label: ReactNode; children: ReactNode }) {
  return (
    <VStack gap={8}>
      <button className="disclosure-label" onClick={onToggle} aria-expanded={expanded}>
        {label}
        <Spacer min={8} />
        <span className={`disclosure-chevron${expanded ? ' open' : ''}`}>
          <Icon name="chevron.right" size={10} weight="semibold" />
        </span>
      </button>
      {expanded ? children : null}
    </VStack>
  )
}

/** Keeps `children` mounted while it fades and folds away. */
export function Reveal({ show, children }: { show: boolean; children: ReactNode }) {
  const { mounted, shown } = usePresence(show)
  if (!mounted) return null
  return (
    <div className={`reveal ${shown ? 'shown' : ''}`}>
      <div>{children}</div>
    </div>
  )
}

/** A filled rounded value box, or outlined (outline or fill, never both). */
export function Pill({ children, fill = 'var(--background)', outlined = false, className }: { children: ReactNode; fill?: string; outlined?: boolean; className?: string }) {
  return (
    <span className={`pill${outlined ? ' outlined' : ''}${className ? ` ${className}` : ''}`} style={{ background: outlined ? undefined : fill }}>
      {children}
    </span>
  )
}

/** Determinate progress (`value` 0...1): a thin capsule track that fills from the left. */
export function ProgressBar({ value, label, fill, className }: { value: number; label: string; fill?: string; className?: string }) {
  return (
    <div className={`progress-track${className ? ` ${className}` : ''}`} role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(value * 100)}>
      <div className="progress-fill" style={{ transform: `scaleX(${value})`, background: fill }} />
    </div>
  )
}

/**
 * A panel's resizable edge: a hairline that drags the panel's width, or steps it with ←/→ while
 * focused. `edge` is the side of the panel it sits on; `onResize` gets the proposed width.
 */
export function ResizeHandle({ label, width, min, max, edge, onResize, onDragging }: {
  label: string
  width: number
  min: number
  max: number
  edge: 'leading' | 'trailing'
  onResize: (width: number) => void
  onDragging?: (dragging: boolean) => void
}) {
  const start = useRef<{ x: number; width: number } | null>(null)
  useEffect(() => () => {
    if (start.current) document.documentElement.classList.remove('col-resizing')
  }, [])
  // Dragging toward the panel's outside grows it.
  const grow = edge === 'trailing' ? 1 : -1
  const end = () => {
    if (!start.current) return
    start.current = null
    document.documentElement.classList.remove('col-resizing')
    onDragging?.(false)
  }
  const onPointerDown = (e: PointerEvent) => {
    if (e.button !== 0) return
    e.currentTarget.setPointerCapture(e.pointerId)
    start.current = { x: e.clientX, width }
    // While dragging, the resize cursor stays wherever the pointer goes.
    document.documentElement.classList.add('col-resizing')
    onDragging?.(true)
  }
  const onPointerMove = (e: PointerEvent) => {
    if (start.current) onResize(start.current.width + grow * (e.clientX - start.current.x))
  }
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return
    e.preventDefault()
    onResize(width + grow * (e.key === 'ArrowRight' ? 16 : -16))
  }
  return (
    <div
      className="resize-handle"
      role="separator"
      tabIndex={0}
      aria-label={label}
      aria-orientation="vertical"
      aria-valuenow={Math.round(width)}
      aria-valuemin={min}
      aria-valuemax={Math.round(max)}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={end}
      onPointerCancel={end}
      onLostPointerCapture={end}
      onKeyDown={onKeyDown}
    />
  )
}
