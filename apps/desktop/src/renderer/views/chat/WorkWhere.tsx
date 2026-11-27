import { whereLine, wherePath, type WhereLine, type WorkTask } from '@shared/work'
import { Icon } from '../../components/Icon'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useCopied } from '../../lib/use-copied'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import './work.css'

/**
 * Where a task's thread works, in one quiet line: the folder symbol and its folder (Own folder for the one Codync made
 * for it), or the branch symbol and its branch. A folder too long for the line is cut in the middle, so its last name
 * stays in view.
 */
export function WorkWhere({ where }: { where: WhereLine }) {
  const tr = useTr()
  const text = where.kind === 'own' ? tr(where.text) : where.text
  const cut = where.kind === 'folder' ? Math.max(text.lastIndexOf('/'), text.lastIndexOf('\\')) : -1
  return (
    <HStack as="span" gap={4} shrink className="work-where">
      <Icon name={where.kind === 'branch' ? 'arrow.triangle.branch' : 'folder'} size={10} color="var(--tertiary)" />
      <HStack as="span" gap={0} shrink>
        {cut > 0 ? <Text size="caption" tone="secondary" truncate className="work-where-lead">{text.slice(0, cut)}</Text> : null}
        <Text size="caption" tone="secondary" truncate className="work-where-end">{cut > 0 ? text.slice(cut) : text}</Text>
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
