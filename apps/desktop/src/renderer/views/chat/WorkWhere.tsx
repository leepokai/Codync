import { whereLine, wherePath, type WhereLine, type WorkTask } from '@shared/work'
import { Icon } from '../../components/Icon'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useCopied } from '../../lib/use-copied'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import './work.css'

/**
 * Where a task's thread works, in one quiet line: the folder symbol and its folder, or the branch symbol and its
 * branch. A folder too long for the line is cut in the middle, so its last name stays in view.
 */
export function WorkWhere({ where }: { where: WhereLine }) {
  const tr = useTr()
  if (where.kind === 'own') return (
    <HStack as="span" gap={4} shrink className="work-where">
      <Icon name="folder" size={10} color="var(--tertiary)" />
      <Text size="caption" tone="secondary" truncate>{tr(where.text)}</Text>
    </HStack>
  )
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

/** Where the task works, as a button that copies the full path and says so for a moment. Nothing for a task whose
 * folder the host doesn't name. */
export function WhereCopy({ task }: { task: WorkTask }) {
  const tr = useTr()
  const store = useStore()
  const where = whereLine(task, store.hello?.home ?? '')
  const path = wherePath(task)
  const [copied, copy] = useCopied()
  if (!where || !path) return null
  return (
    <button
      className="work-where-copy"
      title={tr('Copy path')}
      aria-label={`${tr('Copy path')}: ${path}`}
      onClick={() => copy(path)}
    >
      {copied ? (
        <HStack as="span" gap={4} className="fade-in" role="status">
          <Icon name="checkmark" size={10} color="var(--secondary)" />
          <Text size="caption" tone="secondary">{tr('Path copied')}</Text>
        </HStack>
      ) : (
        <WorkWhere where={where} />
      )}
    </button>
  )
}
