import { useState } from 'react'
import { isWorkingIn, type Entry, type EntryData } from '@shared/models'
import { CardSection, Disclosure } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, IconSlot, VStack } from '../../components/Stack'
import { Text, type Tone } from '../../components/Text'
import { ThinkingOrb, type OrbState } from '../../components/ThinkingOrb'
import type { Weight } from '../../lib/fonts'
import { useStore } from '../../store/context'
import { useScreenViewed } from '../../store/analytics'
import { MarkdownText } from './Markdown'
import { CodeBox, DiffView } from './PermissionCard'
import { useFollowEnd } from './follow-end'
import { runningTurn, traceTurns } from './trace-turns'
import './trace.css'
import { useTr } from '../../lib/use-language'


/**
 * "Full conversation": everything the agent did (narration, thinking, tool calls with output
 * and diffs, plans) grouped by turn.
 */
export function TraceView({ botId }: { botId: string }) {
  const tr = useTr()
  const store = useStore()
  useScreenViewed('trace')
  const all = store.allEntries(botId)
  const sorted = traceTurns(all)
  // The running turn's thoughts start unfolded.
  const bot = store.bots.get(botId)
  const running = runningTurn(all, !!bot && isWorkingIn(bot, botId))
  const follow = useFollowEnd()
  return (
    <SheetPage>
      <ModalHeader title={tr(`Full conversation`)} />
      <div ref={follow.ref} className="thread-scroll" onScroll={follow.onScroll}>
        <VStack gap={14} className="trace-content">
          {sorted.length === 0 ? <Text tone="tertiary">{tr(`Nothing yet.`)}</Text> : null}
          {sorted.map(([turn, entries]) => (
            <VStack key={turn} gap={8}>
              <Text as="div" size="compactSecondary" weight="semibold" tone="text" truncate className="trace-turn-title">
                {entries.find((e) => e.kind === 'user')?.data.text ?? `Turn ${turn}`}
              </Text>
              <CardSection>
                {entries.map((e) => (
                  <TraceRow key={e.id} entry={e} unfolded={turn === running} />
                ))}
              </CardSection>
            </VStack>
          ))}
        </VStack>
      </div>
    </SheetPage>
  )
}

/** An icon centered on the first line of caption text beside it. */
function Label({ icon, weight, tone, children }: { icon: React.ReactNode; weight?: Weight; tone?: Tone; children: React.ReactNode }) {
  return (
    <HStack as="span" align="flex-start" gap={6}>
      <HStack as="span" className="trace-label-icon">{icon}</HStack>
      <Text size="caption" weight={weight} tone={tone} className="trace-label-text">{children}</Text>
    </HStack>
  )
}

/** The running turn's thoughts start `unfolded`, so its reasoning reads as it comes in. */
export function TraceRow({ entry, unfolded }: { entry: Entry; unfolded: boolean }) {
  const tr = useTr()
  const [expanded, setExpanded] = useState(unfolded)
  const d = entry.data
  switch (entry.kind) {
    case 'user':
      return (
        <Label icon={<Icon name="person.fill" size={11} color="var(--secondary)" />} tone="text">
          {d.text ?? ''}
        </Label>
      )
    case 'agent':
      return (
        <VStack gap={4}>
          <Text size="caption2" weight="semibold" tone="tertiary">{d.final ? tr(`Reply`) : tr(`Said`)}</Text>
          <MarkdownText text={d.text ?? ''} />
        </VStack>
      )
    case 'thought':
      return (
        <Disclosure
          expanded={expanded}
          onToggle={() => setExpanded((x) => !x)}
          label={
            <HStack as="span" gap={6}>
              <ThinkingOrb size={16} color="var(--secondary)" animated={false} />
              <Text size="subheadline" tone="secondary">{tr(`Thinking`)}</Text>
            </HStack>
          }
        >
          <Text as="div" size="footnote" tone="secondary" className="selectable trace-thought">{d.text ?? ''}</Text>
        </Disclosure>
      )
    case 'tool':
      return <ToolRow data={d} />
    case 'plan':
      return (
        <VStack gap={6}>
          <Label icon={<Icon name="checklist" size={11} weight="bold" />} weight="bold">
            Plan
          </Label>
          {(d.entries ?? []).map((item, i) => (
            <Label
              key={i}
              tone="text"
              icon={
                <Icon
                  name={item.status === 'completed' ? 'checkmark.circle.fill' : item.status === 'in_progress' ? 'circle.dotted' : 'circle'}
                  size={10}
                  color={item.status === 'completed' ? 'var(--accent)' : 'var(--tertiary)'}
                />
              }
            >
              <span className={item.status === 'completed' ? 'trace-plan-done' : undefined}>{item.content}</span>
            </Label>
          ))}
        </VStack>
      )
    case 'permission':
      return (
        <Label icon={<Icon name="hand.raised" size={11} color="var(--secondary)" />} tone="secondary">
          {d.title ?? 'Approval'}
        </Label>
      )
    default:
      return <Text size="footnote" tone="tertiary">{d.text ?? ''}</Text>
  }
}

const toolIcon = (kind?: string) =>
  ({ read: 'doc.text', edit: 'pencil', delete: 'trash', move: 'arrow.right.doc.on.clipboard', search: 'magnifyingglass', execute: 'terminal', think: 'brain', fetch: 'globe' })[kind ?? ''] ?? 'wrench.and.screwdriver'

const toolOrb = (kind?: string): OrbState => ({ search: 'searching', fetch: 'connecting', edit: 'shaping' } as const)[kind ?? ''] ?? 'working'

export function ToolRow({ data }: { data: EntryData }) {
  const tr = useTr()
  const [expanded, setExpanded] = useState(false)
  const hasBody = !!data.output || !!data.diffs?.length
  const label = (
    <HStack as="span" gap={8} grow>
      <IconSlot width={18} tone="secondary">
        <Icon name={toolIcon(data.toolKind)} size={10} />
      </IconSlot>
      <Text size="subheadline" tone="text" lines={2} grow>{data.title ?? tr(`Tool`)}</Text>
      {data.status === 'completed' ? (
        <Icon name="checkmark" size={10} color="var(--accent)" />
      ) : data.status === 'failed' ? (
        <Icon name="xmark" size={10} color="var(--danger)" />
      ) : (
        <ThinkingOrb state={toolOrb(data.toolKind)} size={16} color="var(--secondary)" />
      )}
    </HStack>
  )
  if (!hasBody) return label
  return (
    <Disclosure expanded={expanded} onToggle={() => setExpanded((x) => !x)} label={label}>
      <VStack gap={8}>
        {(data.diffs ?? []).map((diff, i) => <DiffView key={i} diff={diff} />)}
        {data.output ? <CodeBox text={data.output} /> : null}
      </VStack>
    </Disclosure>
  )
}
