import { useState, type ReactNode } from 'react'
import type { Bot } from '@shared/models'
import { canCutIn, taskLine, pendingAsks, whereLine, wherePath, workSections, type WorkTask } from '@shared/work'
import { BotAvatar } from '../../components/Avatar'
import { Button, IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { CutInDialog, StopWorkDialog, WORK_MARK, WorkDot } from './WorkDialogs'
import { Timer } from './WorkingIndicator'
import { WorkWhere } from './WorkWhere'
import './work.css'

/**
 * What a bot works on now and what waits, with their actions: its threads (tasks that need the user, then the
 * working ones; Stop) and its event box (the waiting ones, numbered in the order they will be taken; First, Cut in,
 * Drop). A row of a task a thread has opens its thread (`openThread`); one that needs the user calls `openCard`.
 *
 * On the bot's Details they are two titled cards, **Threads** (`2 / 3`, with a faint row per thread not in use) and
 * **Event box** (how many wait). `plain` is the view of all bots: one card of the same rows, no titles, no free rows.
 */
export function LiveWork({ bot, openCard, openThread, plain = false }: { bot: Bot; openCard: (taskId: string) => void; openThread: (taskId: string) => void; plain?: boolean }) {
  const tr = useTr()
  const store = useStore()
  const { threads, atOnce, free, box } = workSections(bot.work, bot.maxThreads)
  const asks = pendingAsks(store.allEntries(bot.id), tr)
  const cutIn = canCutIn(bot.work, bot.maxThreads)
  const offline = store.isOffline
  const [stopping, setStopping] = useState<string | null>(null)
  const [cuttingIn, setCuttingIn] = useState<WorkTask | null>(null)
  const stop = (task: WorkTask) => <IconButton title="Stop" icon="stop.fill" size={24} disabled={offline} onClick={() => setStopping(task.id)} />
  const threadRows = threads.map((task) =>
    task.status === 'needsYou' ? (
      <WorkRow key={task.id} task={task} line={asks.get(task.id) || taskLine(task, tr)} open={() => openCard(task.id)} onHover={stop(task)} />
    ) : (
      <WorkRow
        key={task.id}
        task={task}
        line={taskLine(task, tr)}
        open={task.thread ? () => openThread(task.id) : undefined}
        time={task.startedAt !== null ? <Timer since={task.startedAt} /> : null}
        onHover={stop(task)}
      />
    ),
  )
  const boxRows = box.map((task, at) => (
    <WorkRow
      key={task.id}
      task={task}
      number={at + 1}
      line={taskLine(task, tr)}
      always={at === 0 && cutIn ? <Button kind="secondary" className="work-row-button" disabled={offline} onClick={() => setCuttingIn(task)}>Cut in</Button> : null}
      onHover={
        <>
          {at > 0 ? <IconButton title="First" icon="arrow.up.to.line" size={24} disabled={offline} onClick={() => store.workFirst(bot.id, task.id)} /> : null}
          <IconButton title="Drop" icon="xmark" size={24} disabled={offline} onClick={() => store.dropWork(bot.id, task.id)} />
        </>
      }
    />
  ))
  return (
    <>
      {plain ? (
        <VStack className="outlined-card">{threadRows}{boxRows}</VStack>
      ) : (
        <>
          <WorkSection title="Threads" count={`${threads.length} / ${atOnce}`} shown={threads.length + box.length > 0}>
            {threadRows}
            {Array.from({ length: free }, (_, at) => <FreeRow key={`free-${at}`} />)}
          </WorkSection>
          <WorkSection title="Event box" count={String(box.length)} shown={box.length > 0}>{boxRows}</WorkSection>
        </>
      )}
      <StopWorkDialog botId={bot.id} taskId={stopping} onClose={() => setStopping(null)} />
      <CutInDialog bot={bot} task={cuttingIn} onClose={() => setCuttingIn(null)} />
    </>
  )
}

/** A titled card of rows, with `count` in quiet text after the title; nothing unless `shown`. */
export function WorkSection({ title, count, shown, children }: { title: string; count?: string; shown: boolean; children: ReactNode }) {
  const tr = useTr()
  if (!shown) return null
  return (
    <section className="details-section">
      <HStack gap={6} align="baseline">
        <Text className="details-section-title">{tr(title)}</Text>
        {count ? <Text size="callout" tone="secondary" tabular className="work-section-count">{count}</Text> : null}
      </HStack>
      <VStack className="outlined-card">{children}</VStack>
    </section>
  )
}

/** A thread not in use: the ceiling, made visible. */
function FreeRow() {
  const tr = useTr()
  return (
    <HStack gap={10} className="work-row free">
      <WorkDot mark="waiting" />
      <Text tone="tertiary">{tr('Free')}</Text>
    </HStack>
  )
}

/**
 * One task: its mark (or `number`, its place in line), its title over `line` and where its thread works, and `time`. `open` makes the row a button. `always` is an action that
 * always shows (Cut in, Try again); `onHover` holds the icon actions, out of sight until the row is hovered or
 * holds the focus.
 */
export function WorkRow({ task, number, line, time, open, always, onHover }: { task: WorkTask; number?: number; line: string; time?: ReactNode; open?: () => void; always?: ReactNode; onHover?: ReactNode }) {
  const store = useStore()
  const mark = WORK_MARK[task.status]
  const where = whereLine(task, store.hello?.home ?? '')
  const Open = open ? 'button' : 'div'
  return (
    <HStack gap={4} className="work-row" data-work-status={task.status}>
      <Open className="work-row-open" onClick={open}>
        {number ? <Text size="callout" tone="tertiary" tabular className="work-row-number">{number}</Text> : <WorkDot mark={mark} />}
        <VStack as="span" gap={2} align="flex-start" grow shrink>
          <HStack as="span" gap={5} className="work-row-title">
            <SourceMark task={task} />
            <Text tone="text" truncate>{task.title}</Text>
          </HStack>
          <Text size="caption" tone={mark === 'danger' ? 'danger' : mark === 'attention' ? 'attention' : 'secondary'} truncate>{line}</Text>
          {where ? <span className="work-row-where" title={wherePath(task) ?? undefined}><WorkWhere where={where} /></span> : null}
        </VStack>
        {time}
      </Open>
      {always}
      {onHover ? <HStack as="span" gap={2} className="work-row-actions">{onHover}</HStack> : null}
    </HStack>
  )
}

/** Before a task's title, where it came from: a routine's symbol, or the face of the bot that asked. Nothing for
 * what the bot added itself, or a bot this computer doesn't have. */
function SourceMark({ task }: { task: WorkTask }) {
  const tr = useTr()
  const store = useStore()
  if (task.source?.kind === 'routine') return <Icon name="clock.arrow.circlepath" size={11} color="var(--secondary)" title={tr('Routine')} className="work-row-source" />
  const asking = task.source?.id ? store.bots.get(task.source.id) : undefined
  return asking ? <span className="work-row-source" title={asking.name} aria-label={asking.name} role="img"><BotAvatar bot={asking} size={14} animated={false} /></span> : null
}
