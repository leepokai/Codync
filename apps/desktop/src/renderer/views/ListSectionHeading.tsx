import type { ReactNode } from 'react'
import { IconButton } from '../components/Controls'
import { Icon } from '../components/Icon'
import { HStack } from '../components/Stack'
import { Text } from '../components/Text'
import './computer-roster.css'
import './threads-page.css'

/**
 * A group's heading in a page's list (Threads, Workflows): what it is and a mark at its end, with `onFold` a click
 * that folds the group (a chevron at its end, like the sidebar's computer headings), and with `add` a `+` on hover for a new item in it.
 */
export function ListSectionHeading({ leading, title, mark, folded, onFold, add }: {
  leading: ReactNode
  title: string
  mark?: ReactNode
  folded?: boolean
  onFold?: () => void
  add?: { title: string; action: () => void } | null
}) {
  const chevron = onFold ? <span className={`computer-roster-chevron ${folded ? 'collapsed' : ''}`}><Icon name="chevron.down" size={9} /></span> : null
  const content = <>{leading}<Text tone="text" truncate grow>{title}</Text>{mark}{chevron}</>
  return (
    <HStack gap={2} className="thread-section-heading">
      {onFold ? (
        <button className="thread-row thread-section-toggle" aria-expanded={!folded} onClick={onFold}>{content}</button>
      ) : (
        <div className="thread-row thread-section-toggle">{content}</div>
      )}
      {add ? <IconButton title={add.title} icon="plus" size={24} className="thread-section-new" onClick={add.action} /> : null}
    </HStack>
  )
}
