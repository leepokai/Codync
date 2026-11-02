import type { ReactNode } from 'react'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'

const mac = window.codync.platform === 'darwin'

/** A shortcut as the canvas tip shows it: ⌘F on a Mac, Ctrl+F elsewhere. `mod` is the command key. */
export function chord(...keys: string[]): string {
  return keys.map((key) => (key === 'mod' ? (mac ? '⌘' : 'Ctrl') : key)).join(mac ? '' : '+')
}

/** The name of a canvas control, with its shortcut, shown above the control while the pointer is on it. */
export function WorkflowTip({ label, keys, below = false, children }: { label: string; keys?: string; below?: boolean; children: ReactNode }) {
  const tr = useTr()
  return (
    <span className={`workflow-tip${below ? ' below' : ''}`}>
      {children}
      <span className="workflow-tip-bubble" role="tooltip" aria-hidden>
        <Text size="caption" weight="medium">{tr(label)}</Text>
        {keys ? <span className="workflow-tip-key">{keys}</span> : null}
      </span>
    </span>
  )
}
