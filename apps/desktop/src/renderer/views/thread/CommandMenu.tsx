import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { Icon } from '../../components/Icon'
import { usePresence } from '../../components/Overlay'
import { useTr } from '../../lib/use-language'
import { returnRuns, type SlashCommand } from './slash-commands'

/**
 * The `/` list's highlight and keys: ↑/↓ move, Tab or Return picks, Escape hides the list until
 * the draft changes.
 */
export function useCommandMenu(draft: string, commands: SlashCommand[], run: (command: SlashCommand) => void) {
  const [pick, setPick] = useState({ draft: '', index: 0 })
  const [dismissed, setDismissed] = useState<string | null>(null)
  const shown = dismissed === draft ? [] : commands
  const index = pick.draft === draft ? Math.min(pick.index, shown.length - 1) : 0

  /** Handles a key in the field while the list is up; returns whether it did. */
  const onKey = (e: KeyboardEvent, composing: boolean): boolean => {
    const picked = shown[index]
    if (!picked || composing || e.altKey || e.metaKey || e.ctrlKey) return false
    const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0
    const runs = (e.key === 'Tab' && !e.shiftKey) || (e.key === 'Enter' && !e.shiftKey && returnRuns(picked, draft))
    if (!step && !runs && e.key !== 'Escape') return false
    e.preventDefault()
    e.stopPropagation()
    if (step) setPick({ draft, index: (index + step + shown.length) % shown.length })
    else if (runs) run(picked)
    else setDismissed(draft)
    return true
  }

  return { shown, index, hover: (i: number) => setPick({ draft, index: i }), onKey }
}

/** The `/` list over the transcript, above the box: each command's icon, name and what it does. */
export function CommandMenu({ commands, index, hover, pick }: {
  commands: SlashCommand[]
  index: number
  hover: (index: number) => void
  pick: (command: SlashCommand) => void
}) {
  const open = commands.length > 0
  const tr = useTr()
  const { mounted, shown } = usePresence(open)
  const last = useRef(commands)
  if (open) last.current = commands
  const list = open ? commands : last.current
  const rows = useRef<(HTMLButtonElement | null)[]>([])
  // Braces: newer Chromium returns a promise from `scrollIntoView`, which an effect mustn't return.
  useEffect(() => {
    void rows.current[index]?.scrollIntoView({ block: 'nearest' })
  }, [index, list])
  if (!mounted) return null
  return (
    <div className={`command-menu ${shown ? 'shown' : ''}`} inert={!open}>
      <div className="menu-panel" role="listbox" aria-label="Commands">
        {list.map((c, i) => (
          <button
            key={`${c.action.kind}:${c.name}`}
            ref={(el) => { rows.current[i] = el }}
            role="option"
            aria-selected={i === index}
            className={`menu-row ${i === index ? 'highlight' : ''}`}
            title={tr(c.detail || c.title)}
            onMouseEnter={() => hover(i)}
            // Keep the caret in the field.
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => pick(c)}
          >
            <span className="menu-icon"><Icon name={c.icon} size={12} color="var(--secondary)" /></span>
            <span className="command-title">{tr(c.title)}</span>
            <span className={`command-detail ${c.warning ? 'warning' : ''}`}>{tr(c.detail)}</span>
          </button>
        ))}
      </div>
    </div>
  )
}
