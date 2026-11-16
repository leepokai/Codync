import { useEffect, useState } from 'react'
import type { Entry } from '@shared/models'
import { isTold } from '@shared/tasks'
import { isLive, workStatusWord, type WorkItem } from '@shared/work'
import { Button, CardSection, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useFetched } from '../../lib/use-fetched'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import { useStore } from '../../store/context'
import { useFollowEnd } from './follow-end'
import { TraceRow } from './TraceView'
import { runningTurn, traceTurns } from './trace-turns'
import { StopWorkDialog } from './WorkDialogs'
import { Timer } from './WorkingIndicator'
import './trace.css'
import './tasks.css'

/** An item's status in a word, how long its task has run (or ran, once it ended), and the branch of the worktree it
 * works in, when it has one. */
function ItemStatusLine({ item }: { item: WorkItem }) {
  const tr = useTr()
  return (
    <HStack as="span" gap={6}>
      <Text size="callout" tone="secondary">{tr(workStatusWord(item.status))}</Text>
      {item.startedAt !== null ? <Timer since={item.startedAt} until={item.endedAt ?? undefined} /> : null}
      {item.worktree ? (
        <HStack as="span" gap={4} shrink title={item.worktree.path}>
          <Icon name="arrow.triangle.branch" size={11} color="var(--tertiary)" />
          <Text size="callout" tone="secondary" mono truncate className="selectable">{item.worktree.branch}</Text>
        </HStack>
      ) : null}
    </HStack>
  )
}

/**
 * The transcript of the task that has or had `item`, read only: the item's title, status and time, then what the
 * task was told, its messages, thinking and tool calls, fetched as it opens and kept current while it works.
 */
export function TaskDetails({ botId, item: opened }: { botId: string; item: WorkItem }) {
  const store = useStore()
  const taskId = opened.task ?? ''
  useScreenViewed('task', taskId)
  useEffect(() => store.watchTask(taskId), [store, taskId])
  // As the box has it now; an item that has left the box stays as it was opened.
  const item = store.bots.get(botId)?.work.find((i) => i.id === opened.id) ?? opened
  const [attempt, setAttempt] = useState(0)
  const [stopping, setStopping] = useState(false)
  return (
    <SheetPage>
      <TaskTranscript
        // A new attempt starts over.
        key={attempt}
        botId={botId}
        taskId={taskId}
        item={item}
        retry={() => setAttempt((n) => n + 1)}
        actions={isLive(item) ? <IconButton title="Stop" icon="stop.fill" onClick={() => setStopping(true)} /> : null}
      />
      <StopWorkDialog botId={botId} itemId={stopping ? item.id : null} onClose={() => setStopping(false)} />
    </SheetPage>
  )
}

function TaskTranscript({ botId, taskId, item, retry, actions }: { botId: string; taskId: string; item: WorkItem; retry: () => void; actions: React.ReactNode }) {
  const tr = useTr()
  const store = useStore()
  const live = isLive(item)
  // Read again once it ends, for how it ended.
  const { fetched, error } = useFetched(() => store.loadTask(botId, taskId), [store, botId, taskId, live])
  const entries = store.taskEntriesOf(taskId)
  const turns = traceTurns(entries)
  const running = runningTurn(entries, live)
  const follow = useFollowEnd()
  return (
    <>
      <ModalHeader title={item.title} subtitle={<ItemStatusLine item={item} />} trailing={actions} />
      <div ref={follow.ref} className="thread-scroll" onScroll={follow.onScroll}>
        <VStack gap={14} className="trace-content">
          {error ? (
            <VStack gap={10} align="center" className="task-note">
              <Text size="footnote" tone="danger">{error}</Text>
              <Button kind="secondary" onClick={retry}>Try again</Button>
            </VStack>
          ) : null}
          {!error && fetched === null && entries.length === 0 ? <div className="task-note" aria-label={tr('Loading')}><Spinner /></div> : null}
          {!error && fetched !== null && entries.length === 0 ? <Text as="div" size="footnote" tone="tertiary" className="task-note">{tr('Nothing yet.')}</Text> : null}
          {turns.map(([turn, rows]) => (
            <CardSection key={turn}>
              {rows.map((e) => <TaskRow key={e.id} entry={e} unfolded={turn === running} />)}
            </CardSection>
          ))}
        </VStack>
      </div>
    </>
  )
}

/** A trace row; what the task was told reads as a quote, not as something the user typed. */
function TaskRow({ entry, unfolded }: { entry: Entry; unfolded: boolean }) {
  if (isTold(entry)) return <Text as="div" size="footnote" tone="secondary" className="selectable task-told">{entry.data.text ?? ''}</Text>
  return <TraceRow entry={entry} unfolded={unfolded} />
}
