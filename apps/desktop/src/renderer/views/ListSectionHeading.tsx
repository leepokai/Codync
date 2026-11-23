import type { ReactNode } from 'react'
import { IconButton } from '../components/Controls'
import { HStack } from '../components/Stack'
import { Text } from '../components/Text'
import './threads-page.css'

/**
 * A group's heading in a page's list (Threads, Workflows): what it is and a mark at its end, with `onFold` a click
 * that folds the group, and with `add` a `+` on hover for a new item in it.
 */
export function ListSectionHeading({ leading, title, mark, folded, onFold, add }: {
  leading: ReactNode
  title: string
  mark?: ReactNode
  folded?: boolean
  onFold?: () => void
  add?: { title: string; action: () => void } | null
}) {
  const content = <>{leading}<Text tone="text" truncate grow>{title}</Text>{mark}</>
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
