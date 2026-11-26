import { useState, type CSSProperties, type PointerEvent } from 'react'
import type { ModelOption } from '@shared/agent-models'

/** Where the first and last steps' centers sit from the track's ends (half a step in from its padding). */
const INSET = 13

/**
 * The effort as a track of steps: a step is picked by a click, or by dragging the knob along the track.
 * A drag picks each step it passes once; the knob follows it before the pick comes back.
 */
export function EffortTrack({ option, chosen, onPick }: { option: ModelOption; chosen: string | undefined; onPick: (id: string) => void }) {
  const steps = option.values
  const [dragged, setDragged] = useState<number | null>(null)
  const at = dragged ?? Math.max(0, steps.findIndex((v) => v.id === chosen))
  const pickAt = (e: PointerEvent<HTMLDivElement>) => {
    const box = e.currentTarget.getBoundingClientRect()
    const ratio = Math.min(1, Math.max(0, (e.clientX - box.left - INSET) / (box.width - 2 * INSET)))
    const index = Math.round(ratio * (steps.length - 1))
    const step = steps[index]
    if (!step || index === at) return
    setDragged(index)
    onPick(step.id)
  }
  return (
    <div
      className="model-effort"
      role="radiogroup"
      aria-label={option.name}
      style={{ '--at': at / (steps.length - 1), '--inset': `${INSET}px` } as CSSProperties}
      onPointerDown={(e) => {
        if (e.button !== 0) return
        e.currentTarget.setPointerCapture(e.pointerId)
        pickAt(e)
      }}
      onPointerMove={(e) => {
        if (e.currentTarget.hasPointerCapture(e.pointerId)) pickAt(e)
      }}
      onLostPointerCapture={() => setDragged(null)}
    >
      <span className="model-effort-fill" />
      <span className="model-effort-knob" />
      {steps.map((v) => (
        // The pointer picks on the track; a click here is the keyboard's (Space or Enter).
        <button key={v.id} role="radio" aria-checked={v.id === chosen} title={v.name} className="model-effort-step" onClick={(e) => (e.detail === 0 ? onPick(v.id) : undefined)}>
          <span className="model-effort-dot" />
        </button>
      ))}
    </div>
  )
}
