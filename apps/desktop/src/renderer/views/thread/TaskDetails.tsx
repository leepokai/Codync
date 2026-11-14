import { useEffect, useState } from 'react'
import type { Entry } from '@shared/models'
import { isTold, taskStatusWord, type BotTask } from '@shared/tasks'
import { Button, CardSection, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog, ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useFetched } from '../../lib/use-fetched'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import { useStore } from '../../store/context'
import { useFollowEnd } from './follow-end'
import { TraceRow } from './TraceView'
import { runningTurn, traceTurns } from './trace-turns'
import { Timer } from './WorkingIndicator'
import './trace.css'
import './tasks.css'

/** Asks once before stopping the bot's background work: the one task, or all of them without `taskId`. */
export function StopTaskDialog({ botId, taskId, open, onClose }: { botId: string; taskId?: string; open: boolean; onClose: () => void }) {
  const tr = useTr()
  const store = useStore()
  return (
    <Dialog
      open={open}
      title={tr('Stop the background work?')}
      message={tr('What it already changed stays.')}
      actions={[{ title: 'Stop', action: () => store.stopTask(botId, taskId) }]}
      onClose={onClose}
    />
  )
}

/** What the capsule opens: the one task's details, or with several a list that leads to each. */
export function TasksView({ botId }: { botId: string }) {
  const tasks = useStore().bots.get(botId)?.tasks ?? []
  // Chosen as the sheet opens: a task that ends while it's read stays on screen.
  const [only] = useState(() => (tasks.length === 1 ? tasks[0]!.id : null))
  const [picked, setPicked] = useState<string | null>(null)
  const shown = only ?? picked
  if (shown) return <TaskDetails botId={botId} taskId={shown} back={only ? undefined : () => setPicked(null)} />
  return <TaskList tasks={tasks} open={setPicked} />
}

function TaskList({ tasks, open }: { tasks: BotTask[]; open: (taskId: string) => void }) {
  const tr = useTr()
  return (
    <SheetPage>
      <ModalHeader title={tr('In the background')} />
      <div className="thread-scroll">
        <VStack className="trace-content">
          {tasks.length === 0 ? <Text tone="tertiary">{tr('Nothing yet.')}</Text> : (
            <CardSection>
              {tasks.map((task) => (
                <button key={task.id} className="task-list-row" onClick={() => open(task.id)}>
                  <VStack gap={2} align="flex-start" grow shrink>
                    <Text size="body" weight="medium" tone="text" truncate>{task.title}</Text>
                    <TaskStatusLine status={task.status} startedAt={task.startedAt} />
                  </VStack>
                  <Icon name="chevron.right" size={10} color="var(--tertiary)" />
                </button>
              ))}
            </CardSection>
          )}
        </VStack>
      </div>
    </SheetPage>
  )
}

/** A task's status in words and how long it has run (or ran, once it ended). */
function TaskStatusLine({ status, startedAt, endedAt }: { status: Parameters<typeof taskStatusWord>[0]; startedAt: number; endedAt?: number }) {
  const tr = useTr()
  return (
    <HStack as="span" gap={6}>
      <Text size="callout" tone="secondary">{tr(taskStatusWord(status))}</Text>
      <Timer since={startedAt} until={endedAt} />
    </HStack>
  )
}

/**
 * One background task, read only: its title, status and time, and its own transcript (what it was told, its
 * messages, thinking and tool calls), fetched as it opens and kept current while it works.
 */
function TaskDetails({ botId, taskId, back }: { botId: string; taskId: string; back?: () => void }) {
  const store = useStore()
  useScreenViewed('task', taskId)
  useEffect(() => store.watchTask(taskId), [store, taskId])
  const live = store.bots.get(botId)?.tasks.find((t) => t.id === taskId)
  const [attempt, setAttempt] = useState(0)
  const [stopping, setStopping] = useState(false)
  return (
    <SheetPage>
      <TaskTranscript
        // A new attempt starts over.
        key={attempt}
        botId={botId}
        taskId={taskId}
        live={live}
        retry={() => setAttempt((n) => n + 1)}
        actions={
          <>
            {back ? <IconButton title="Back" icon="chevron.left" onClick={back} /> : null}
            {live ? <IconButton title="Stop" icon="stop.fill" onClick={() => setStopping(true)} /> : null}
          </>
        }
      />
      <StopTaskDialog botId={botId} taskId={taskId} open={stopping} onClose={() => setStopping(false)} />
    </SheetPage>
  )
}

function TaskTranscript({ botId, taskId, live, retry, actions }: { botId: string; taskId: string; live: BotTask | undefined; retry: () => void; actions: React.ReactNode }) {
  const tr = useTr()
  const store = useStore()
  // Read again once it ends, for how it ended.
  const { fetched: record, error } = useFetched(() => store.loadTask(botId, taskId), [store, botId, taskId, !live])
  const entries = store.taskEntriesOf(taskId)
  const turns = traceTurns(entries)
  const running = runningTurn(entries, !!live)
  const follow = useFollowEnd()
  const status = live?.status ?? record?.status
  const startedAt = live?.startedAt ?? record?.startedAt
  return (
    <>
      <ModalHeader
        title={live?.title ?? record?.title ?? tr('In the background')}
        subtitle={status && startedAt ? <TaskStatusLine status={status} startedAt={startedAt} endedAt={live ? undefined : record?.endedAt ?? undefined} /> : undefined}
        trailing={actions}
      />
      <div ref={follow.ref} className="thread-scroll" onScroll={follow.onScroll}>
        <VStack gap={14} className="trace-content">
          {error ? (
            <VStack gap={10} align="center" className="task-note">
              <Text size="footnote" tone="danger">{error}</Text>
              <Button kind="secondary" onClick={retry}>Try again</Button>
            </VStack>
          ) : null}
          {!error && record === null && entries.length === 0 ? <div className="task-note" aria-label={tr('Loading')}><Spinner /></div> : null}
          {!error && record !== null && entries.length === 0 ? <Text as="div" size="footnote" tone="tertiary" className="task-note">{tr('Nothing yet.')}</Text> : null}
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
