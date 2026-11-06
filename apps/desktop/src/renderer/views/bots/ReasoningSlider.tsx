import { useId, type CSSProperties } from 'react'
import { Icon } from '../../components/Icon'
import { useTr } from '../../lib/use-language'
import type { ModelOption } from './agent-models'

export const isReasoningOption = (option: ModelOption) => option.kind === 'reasoning'
  || ['effort', 'reasoning_effort', 'reasoning-effort', 'thought_level'].includes(option.id)

/** Discrete agent-advertised levels, with native range keyboard and assistive-technology support. */
export function ReasoningSlider({ option, value, onChange, onReset }: {
  option: ModelOption
  value?: string
  onChange: (value: string) => void
  onReset: () => void
}) {
  const tr = useTr()
  const label = useId()
  const index = option.values.findIndex((v) => v.id === (value ?? option.default))
  const last = option.values.length - 1
  const name = index >= 0 ? option.values[index]!.name : ''
  if (last < 0) return null
  return <div className="reasoning-control">
    <div className="reasoning-heading">
      <span id={label}>{option.name}</span>
      <strong className="reasoning-value"><span key={name} className="agent-picker-text-change">{name}</span></strong>
      <button className="agent-picker-default" aria-pressed={value === undefined} onClick={onReset}>
        {tr('Default')}<Icon name={value === undefined ? 'checkmark' : 'arrow.counterclockwise'} size={10} />
      </button>
    </div>
    <div className={`reasoning-slider${index < 0 ? ' unknown' : ''}`} style={{ '--effort-position': last > 0 ? Math.max(0, index) / last : 0 } as CSSProperties}>
      <div className="reasoning-track" aria-hidden="true"><div className="reasoning-fill" /></div>
      <div className="reasoning-stops" aria-hidden="true">{option.values.map((level) => <span key={level.id} title={level.name} />)}</div>
      <div className="reasoning-thumb" aria-hidden="true" />
      <input type="range" min={0} max={Math.max(1, last)} step={1} value={Math.max(0, index)}
        onPointerDown={(event) => {
          if (index >= 0 && last > 0) return
          const bounds = event.currentTarget.getBoundingClientRect()
          const fraction = Math.max(0, Math.min(1, (event.clientX - bounds.left - 15) / Math.max(1, bounds.width - 30)))
          const level = option.values[Math.round(fraction * last)]
          if (level) onChange(level.id)
        }}
        onKeyDown={(event) => {
          if ((index < 0 || last === 0) && ['Home', 'ArrowLeft', 'ArrowDown'].includes(event.key)) {
            event.preventDefault()
            onChange(option.values[0]!.id)
          }
        }}
        aria-labelledby={label} aria-valuetext={name || tr('Default')}
        onChange={(event) => { const level = option.values[Number(event.target.value)]; if (level) onChange(level.id) }} />
    </div>
  </div>
}
