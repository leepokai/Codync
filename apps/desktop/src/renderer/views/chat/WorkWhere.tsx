import type { WhereLine } from '@shared/work'
import { Icon } from '../../components/Icon'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import './work.css'

/**
 * Where a task's thread works, in one quiet line: the folder symbol and its folder, or the branch symbol and its
 * branch. A folder too long for the line is cut in the middle, so its last name stays in view.
 */
export function WorkWhere({ where }: { where: WhereLine }) {
  const cut = where.kind === 'folder' ? Math.max(where.text.lastIndexOf('/'), where.text.lastIndexOf('\\')) : -1
  return (
    <HStack as="span" gap={4} shrink className="work-where">
      <Icon name={where.kind === 'branch' ? 'arrow.triangle.branch' : 'folder'} size={10} color="var(--tertiary)" />
      <HStack as="span" gap={0} shrink>
        {cut > 0 ? <Text size="caption" tone="secondary" truncate className="work-where-lead">{where.text.slice(0, cut)}</Text> : null}
        <Text size="caption" tone="secondary" truncate className="work-where-end">{cut > 0 ? where.text.slice(cut) : where.text}</Text>
      </HStack>
    </HStack>
  )
}
