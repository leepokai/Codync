import { useState, type ReactNode } from 'react'
import type { Bot } from '@shared/models'
import { canCutIn, itemLine, pendingAsks, workSections, type WorkItem } from '@shared/work'
import { Button, IconButton } from '../../components/Controls'
import { Sheet } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { TaskDetails } from './TaskDetails'
import { CutInDialog, StopWorkDialog, WORK_MARK, WorkDot } from './WorkDialogs'
import { Timer } from './WorkingIndicator'
import './work.css'

/**
 * What the bot does, on its Details: **Threads** (the agent sessions that run now, of the most allowed: items that
 * need the user, then the working ones, then a faint row per place not in use), **Event box** (what waits for a
 * thread, numbered in the order it will be taken), then `children` (what it does later and on a schedule), then
 * **Recent** (items that ended in the last day). Threads shows only while an item is live or waiting; the others are
 * left out with no item. A row of an item a task has or had opens that task's transcript; one that needs the user opens the
 * chat at its card (`openCard`).
 */
export function WorkTimeline({ bot, openCard, children }: { bot: Bot; openCard: (itemId: string) => void; children: ReactNode }) {
  const tr = useTr()
  const store = useStore()
  const { threads, places, free, box, recent } = workSections(bot.work, bot.maxAgents)
  const asks = pendingAsks(store.allEntries(bot.id))
  const cutIn = canCutIn(bot.work, bot.maxAgents)
  const offline = store.isOffline
  const [reading, setReading] = useState<WorkItem | null>(null)
  const [stopping, setStopping] = useState<string | null>(null)
  const [cuttingIn, setCuttingIn] = useState<WorkItem | null>(null)
  const read = (item: WorkItem) => (item.task ? () => setReading(item) : undefined)
  const stop = (item: WorkItem) => <IconButton title="Stop" icon="stop.fill" size={24} disabled={offline} onClick={() => setStopping(item.id)} />
  return (
    <>
      <WorkSection title="Threads" count={`${threads.length} / ${places}`} shown={threads.length + box.length > 0}>
        {threads.map((item) =>
          item.status === 'needsYou' ? (
            <WorkRow key={item.id} item={item} line={asks.get(item.id) || itemLine(item, tr)} open={() => openCard(item.id)} onHover={stop(item)} />
          ) : (
            <WorkRow
              key={item.id}
              item={item}
              line={itemLine(item, tr)}
              open={read(item)}
              time={item.startedAt !== null ? <Timer since={item.startedAt} /> : null}
              onHover={stop(item)}
            />
          ),
        )}
        {Array.from({ length: free }, (_, place) => <FreeRow key={`free-${place}`} />)}
      </WorkSection>
      <WorkSection title="Event box" count={String(box.length)} shown={box.length > 0}>
        {box.map((item, at) => (
          <WorkRow
            key={item.id}
            item={item}
            number={at + 1}
            line={itemLine(item, tr)}
            always={at === 0 && cutIn ? <Button kind="secondary" className="work-row-button" disabled={offline} onClick={() => setCuttingIn(item)}>Cut in</Button> : null}
            onHover={
              <>
                {at > 0 ? <IconButton title="First" icon="arrow.up.to.line" size={24} disabled={offline} onClick={() => store.workFirst(bot.id, item.id)} /> : null}
                <IconButton title="Drop" icon="xmark" size={24} disabled={offline} onClick={() => store.dropWork(bot.id, item.id)} />
              </>
            }
          />
        ))}
      </WorkSection>
      {children}
      <WorkSection title="Recent" shown={recent.length > 0}>
        {recent.map((item) => (
          <WorkRow
            key={item.id}
            item={item}
            line={itemLine(item, tr)}
            open={read(item)}
            time={item.status === 'done' && item.startedAt !== null && item.endedAt !== null ? <Timer since={item.startedAt} until={item.endedAt} /> : null}
            always={item.status === 'done' ? null : <IconButton title="Try again" icon="arrow.clockwise" size={24} disabled={offline} onClick={() => store.retryWork(bot.id, item.id)} />}
            onHover={<IconButton title="Drop" icon="xmark" size={24} disabled={offline} onClick={() => store.dropWork(bot.id, item.id)} />}
          />
        ))}
      </WorkSection>
      <StopWorkDialog botId={bot.id} itemId={stopping} onClose={() => setStopping(null)} />
      <CutInDialog bot={bot} item={cuttingIn} onClose={() => setCuttingIn(null)} />
      <Sheet open={reading !== null} onClose={() => setReading(null)} width={620} height={560}>
        {reading ? <TaskDetails botId={bot.id} item={reading} /> : null}
      </Sheet>
    </>
  )
}

/** A titled card of rows, with `count` in quiet text after the title; nothing unless `shown`. */
function WorkSection({ title, count, shown, children }: { title: string; count?: string; shown: boolean; children: ReactNode }) {
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

/** A place no thread uses: the ceiling, made visible. */
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
 * One item: its mark (or `number`, its place in line), its title over `line`, and `time`. `open` makes the row a button. `always` is an action that
 * always shows (Cut in, Try again); `onHover` holds the icon actions, out of sight until the row is hovered or
 * holds the focus.
 */
function WorkRow({ item, number, line, time, open, always, onHover }: { item: WorkItem; number?: number; line: string; time?: ReactNode; open?: () => void; always?: ReactNode; onHover?: ReactNode }) {
  const mark = WORK_MARK[item.status]
  const Open = open ? 'button' : 'div'
  return (
    <HStack gap={4} className="work-row" data-work-status={item.status}>
      <Open className="work-row-open" onClick={open}>
        {number ? <Text size="callout" tone="tertiary" tabular className="work-row-number">{number}</Text> : <WorkDot mark={mark} />}
        <VStack as="span" gap={2} align="flex-start" grow shrink>
          <Text tone="text" truncate>{item.title}</Text>
          <Text size="caption" tone={mark === 'danger' ? 'danger' : mark === 'attention' ? 'attention' : 'secondary'} truncate>{line}</Text>
        </VStack>
        {time}
      </Open>
      {always}
      {onHover ? <HStack as="span" gap={2} className="work-row-actions">{onHover}</HStack> : null}
    </HStack>
  )
}
