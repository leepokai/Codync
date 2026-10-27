import { useId, useLayoutEffect, useRef, type ReactNode } from 'react'
import { CharacterAvatar } from '../../components/Avatar'
import { RowLabel, Switch } from '../../components/Controls'
import { VStack } from '../../components/Stack'
import { useTr } from '../../lib/use-language'
import { AVATAR_COLORS, AVATAR_SHAPES } from '../../lib/theme'
import './bots.css'

/** A labeled input inside a section row: label (and an optional note) over the field, which they name. */
export function Field({ label, detail, children }: { label: string; detail?: string | null; children: ReactNode }) {
  const id = useId()
  return (
    <VStack gap={8} role="group" aria-labelledby={`${id}-label`} aria-describedby={detail ? `${id}-detail` : undefined}>
      <RowLabel label={label} detail={detail} labelId={`${id}-label`} detailId={`${id}-detail`} />
      {children}
    </VStack>
  )
}

/** An on/off row: title and a short note, the switch on the right. */
export function SwitchRow({ title, detail, on, onChange }: { title: string; detail?: string | null; on: boolean; onChange: (on: boolean) => void }) {
  return (
    <Switch on={on} onChange={onChange}>
      <RowLabel label={title} detail={detail} />
    </Switch>
  )
}

/** A multi-line field that grows with its text between `minRows` and `maxRows` (`axis: .vertical`). */
export function AutoTextArea({ value, onChange, placeholder, minRows, maxRows, className, onKeyDown }: {
  value: string
  onChange: (v: string) => void
  placeholder?: string
  minRows: number
  maxRows: number
  className?: string
  onKeyDown?: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void
}) {
  const tr = useTr()
  const ref = useRef<HTMLTextAreaElement>(null)
  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    const style = getComputedStyle(el)
    const line = parseFloat(style.lineHeight) || 15
    const pad = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom)
    el.style.height = '0px'
    el.style.height = `${Math.min(Math.max(el.scrollHeight, line * minRows + pad), line * maxRows + pad)}px`
  }, [value, minRows, maxRows])
  return <textarea ref={ref} rows={minRows} className={className} value={value} placeholder={placeholder === undefined ? undefined : tr(placeholder)} onChange={(e) => onChange(e.target.value)} onKeyDown={onKeyDown} />
}

/** The avatar's shape and color: each a wrapping, centered row of choices, ringed when picked. */
export function AvatarPicker({ shape, color, onShape, onColor }: { shape: string; color: string; onShape: (s: string) => void; onColor: (c: string) => void }) {
  return (
    <VStack gap={12} className="avatar-picker">
      <div className="swatch-row">
        {AVATAR_SHAPES.map((s) => (
          <button key={s} className={`swatch ${s === shape ? 'on' : ''}`} aria-label={`${s} shape`} aria-pressed={s === shape} onClick={() => onShape(s)}>
            <CharacterAvatar shape={s} color={color} size={36} />
          </button>
        ))}
      </div>
      <div className="swatch-row">
        {AVATAR_COLORS.map((c) => (
          <button key={c.id} className={`swatch ${c.id === color ? 'on' : ''}`} aria-label={c.label} aria-pressed={c.id === color} onClick={() => onColor(c.id)}>
            <span className="swatch-color" style={{ background: c.hex }} />
          </button>
        ))}
      </div>
    </VStack>
  )
}

export { AnchoredPanel } from '../../components/AnchoredPanel'
