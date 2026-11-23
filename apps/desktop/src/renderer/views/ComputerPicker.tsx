import { ChoicePicker } from '../components/Controls'
import { useApp } from '../store/context'
import type { ComputerChoice } from './computer-choice'
import { ComputerBadge } from './ComputerBadge'

/** A page's computer menu: each computer by its badge and name. */
export function ComputerPicker({ choice }: { choice: ComputerChoice }) {
  const app = useApp()
  return (
    <ChoicePicker
      selection={choice.selection}
      options={choice.options.map((o) => ({ ...o, leading: <ComputerBadge computer={app.store(o.id)?.computer ?? null} size={16} /> }))}
      onChange={choice.onChange}
      fill="var(--field)"
    />
  )
}
