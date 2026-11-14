import { ProgressBar } from '../../components/Controls'
import './usage.css'

/** A capsule filled to `percent`: the provider's tint below 70%, then amber, red from 90%. */
export function UsageBar({ percent, tint }: { percent: number; tint: string }) {
  const fill = percent >= 90 ? 'var(--danger)' : percent >= 70 ? 'var(--warning)' : tint
  return (
    <ProgressBar
      value={Math.min(1, Math.max(0.02, percent / 100))}
      label={`${Math.round(percent)}% used`}
      fill={fill}
      className="usage-bar"
    />
  )
}
