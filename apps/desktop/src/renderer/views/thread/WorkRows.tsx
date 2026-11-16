import { useState, type ReactNode } from 'react'
import type { Bot } from '@shared/models'
import { canCutIn, itemLine, pendingAsks, whereLine, wherePath, workSections, type WorkItem } from '@shared/work'
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
 * What a bot works on now and what waits, with their actions: its threads (items that need the user, then the
 * working ones; Stop) and its event box (the waiting ones, numbered in the order they will be taken; First, Cut in,
 * Drop). A row of an item a task has opens its thread (`openThread`); one that needs the user calls `openCard`.
 *
 * On the bot's Details they are two titled cards, **Threads** (`2 / 3`, with a faint row per place not in use) and
 * **Event box** (how many wait). `plain` is the view of all bots: one card of the same rows, no titles, no free places.
 */
export function LiveWork({ bot, openCard, openThread, plain = false }: { bot: Bot; openCard: (itemId: string) => void; openThread: (itemId: string) => void; plain?: boolean }) {
  const tr = useTr()
  const store = useStore()
  const { threads, places, free, box } = workSections(bot.work, bot.maxAgents)
  const asks = pendingAsks(store.allEntries(bot.id), tr)
  const cutIn = canCutIn(bot.work, bot.maxAgents)
  const offline = store.isOffline
  const [stopping, setStopping] = useState<string | null>(null)
  const [cuttingIn, setCuttingIn] = useState<WorkItem | null>(null)
  const stop = (item: WorkItem) => <IconButton title="Stop" icon="stop.fill" size={24} disabled={offline} onClick={() => setStopping(item.id)} />
  const threadRows = threads.map((item) =>
    item.status === 'needsYou' ? (
      <WorkRow key={item.id} item={item} line={asks.get(item.id) || itemLine(item, tr)} open={() => openCard(item.id)} onHover={stop(item)} />
    ) : (
      <WorkRow
        key={item.id}
        item={item}
        line={itemLine(item, tr)}
        open={item.task ? () => openThread(item.id) : undefined}
        time={item.startedAt !== null ? <Timer since={item.startedAt} /> : null}
        onHover={stop(item)}
      />
    ),
  )
  const boxRows = box.map((item, at) => (
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
  ))
  return (
    <>
      {plain ? (
        <VStack className="outlined-card">{threadRows}{boxRows}</VStack>
      ) : (
        <>
          <WorkSection title="Threads" count={`${threads.length} / ${places}`} shown={threads.length + box.length > 0}>
            {threadRows}
            {Array.from({ length: free }, (_, place) => <FreeRow key={`free-${place}`} />)}
          </WorkSection>
          <WorkSection title="Event box" count={String(box.length)} shown={box.length > 0}>{boxRows}</WorkSection>
        </>
      )}
      <StopWorkDialog botId={bot.id} itemId={stopping} onClose={() => setStopping(null)} />
      <CutInDialog bot={bot} item={cuttingIn} onClose={() => setCuttingIn(null)} />
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
 * One item: its mark (or `number`, its place in line), its title over `line` and where its task works, and `time`. `open` makes the row a button. `always` is an action that
 * always shows (Cut in, Try again); `onHover` holds the icon actions, out of sight until the row is hovered or
 * holds the focus.
 */
export function WorkRow({ item, number, line, time, open, always, onHover }: { item: WorkItem; number?: number; line: string; time?: ReactNode; open?: () => void; always?: ReactNode; onHover?: ReactNode }) {
  const store = useStore()
  const mark = WORK_MARK[item.status]
  const where = whereLine(item, store.hello?.home ?? '')
  const Open = open ? 'button' : 'div'
  return (
    <HStack gap={4} className="work-row" data-work-status={item.status}>
      <Open className="work-row-open" onClick={open}>
        {number ? <Text size="callout" tone="tertiary" tabular className="work-row-number">{number}</Text> : <WorkDot mark={mark} />}
        <VStack as="span" gap={2} align="flex-start" grow shrink>
          <HStack as="span" gap={5} className="work-row-title">
            <SourceMark item={item} />
            <Text tone="text" truncate>{item.title}</Text>
          </HStack>
          <Text size="caption" tone={mark === 'danger' ? 'danger' : mark === 'attention' ? 'attention' : 'secondary'} truncate>{line}</Text>
          {where ? <span className="work-row-where" title={wherePath(item) ?? undefined}><WorkWhere where={where} /></span> : null}
        </VStack>
        {time}
      </Open>
      {always}
      {onHover ? <HStack as="span" gap={2} className="work-row-actions">{onHover}</HStack> : null}
    </HStack>
  )
}

/** Before an item's title, where it came from: a routine's symbol, or the face of the bot that asked. Nothing for
 * what the bot added itself, or a bot this computer doesn't have. */
function SourceMark({ item }: { item: WorkItem }) {
  const tr = useTr()
  const store = useStore()
  if (item.source?.kind === 'routine') return <Icon name="clock.arrow.circlepath" size={11} color="var(--secondary)" title={tr('Routine')} className="work-row-source" />
  const asking = item.source?.id ? store.bots.get(item.source.id) : undefined
  return asking ? <span className="work-row-source" title={asking.name} aria-label={asking.name} role="img"><BotAvatar bot={asking} size={14} animated={false} /></span> : null
}
