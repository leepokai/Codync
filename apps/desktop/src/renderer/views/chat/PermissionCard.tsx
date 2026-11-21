import { useState } from 'react'
import type { Entry, FileDiff, PermissionOption } from '@shared/models'
import { Reveal, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import './permission-card.css'
import { OptionBadge } from './DecisionCard'
import { useTr } from '../../lib/use-language'
import { trNow } from '../../lib/l10n.ts'
import { fill } from '@shared/fill'
import { requestDetail, requestIcon, requestName } from './requests'


/** What the agent wants, from the tool's kind (also the notch island's headline). */
export const permissionHeadline = (toolKind?: string) => {
  switch (toolKind) {
    case 'execute':
      return trNow('Wants to run a command')
    case 'edit':
    case 'delete':
    case 'move':
      return trNow('Wants to change files')
    case 'fetch':
      return trNow('Wants to access the web')
    case 'read':
    case 'search':
      return trNow('Wants to read files')
    default:
      return trNow('Wants to use a tool')
  }
}

const rank: Record<string, number> = { allow_once: 0, allow_always: 1, reject_once: 2, reject_always: 3 }

/** Allow before deny, once before always. */
export const sortedOptions = (options: PermissionOption[]) => [...options].sort((a, b) => (rank[a.kind] ?? 9) - (rank[b.kind] ?? 9))

export function optionLabel(o: PermissionOption) {
  switch (o.kind) {
    case 'allow_once':
      return trNow('Allow once')
    case 'allow_always':
      return trNow('Always allow')
    case 'reject_once':
      return trNow('Deny')
    case 'reject_always':
      return trNow('Never')
    default:
      return o.name
  }
}

/** Under what a thread put in the chat (its approval or choice card, a file, an artifact): that it comes from the bot's
 * work, and which task. Nothing for the chat's own. */
export function ThreadLine({ entry }: { entry: Entry }) {
  const tr = useTr()
  const thread = entry.data.thread
  return thread ? <Text as="div" size="caption" tone="tertiary" truncate className="thread-line">{tr('Work')} · {thread.title}</Text> : null
}


/**
 * An approval in the chat. While it waits: what the agent wants and where it runs, the request itself in a well, its
 * details on demand, then the answers in a row (allowing first, refusing last and quiet). Once it is settled it folds
 * to one line, the request and how it ended, which opens to where it ran and what it asked with (no card: there is
 * nothing left to answer); several in a row that ended alike fold into one line of their own (`RequestRun`).
 */
export function PermissionCard({ entry, hostName, answering, respond }: { entry: Entry; hostName: string; answering: string | null; respond: (option: string | null) => void }) {
  const tr = useTr()
  const [expanded, setExpanded] = useState(false)
  const [open, setOpen] = useState(false)
  const d = entry.data
  const pending = d.status === 'pending'
  const detail = requestDetail(d.detail)
  const hasDetail = !!d.command || !!detail || !!d.diffs?.length
  const options = sortedOptions(d.options ?? [])
  const cwd = d.cwd ? ` · ${d.cwd.split('/').filter(Boolean).pop() ?? d.cwd}` : ''
  const where = fill(tr('Runs on {name}{path}'), { name: hostName, path: cwd })
  // Settled, there is nothing to answer: no card, one line that opens to where it ran and what it asked with.
  if (!pending) {
    return (
      <div className="agent-inset">
        <RequestLine icon={requestIcon(d.toolKind, d.title)} title={d.title ?? ''} end={outcome(entry)} expanded={open} onClick={() => setOpen((o) => !o)} />
        <Reveal show={open}>
          <VStack gap={6} className="request-detail">
            <Text size="caption" tone="tertiary">{where}</Text>
            {d.command ? <CodeBox text={d.command} /> : null}
            {detail ? <CodeBox text={detail.slice(0, 2000)} /> : null}
            {(d.diffs ?? []).map((diff, i) => <DiffView key={i} diff={diff} />)}
          </VStack>
        </Reveal>
      </div>
    )
  }
  return (
    <div className="agent-inset">
      <VStack gap={14} className="permission-card decision-card approval-card">
        <VStack gap={4}>
          <Text size="headline" weight="semibold" tone="text">{permissionHeadline(d.toolKind)}</Text>
          {/* Where it runs, and the request in full on demand: one quiet line under the question, Details at its end. */}
          <HStack gap={8} className="approval-card-meta">
            <Text size="caption" tone="tertiary" truncate grow>{where}</Text>
            {hasDetail ? (
              <button className="approval-card-details" aria-expanded={expanded} onClick={() => setExpanded((e) => !e)}>
                <Text size="caption" weight="medium" tone="secondary">{tr('Details')}</Text>
                <span className={`disclosure-chevron${expanded ? ' open' : ''}`}><Icon name="chevron.right" size={8} weight="semibold" /></span>
              </button>
            ) : null}
          </HStack>
          <Text as="div" size="subheadline" mono tone="secondary" lines={expanded ? undefined : 3} className="selectable permission-card-title">
            {d.title ?? ''}
          </Text>
          <ThreadLine entry={entry} />
        </VStack>
        <Reveal show={expanded && hasDetail}>
          <VStack gap={8}>
            {d.command ? <CodeBox text={d.command} /> : null}
            {detail ? <CodeBox text={detail.slice(0, 2000)} /> : null}
            {(d.diffs ?? []).map((diff, i) => <DiffView key={i} diff={diff} />)}
          </VStack>
        </Reveal>
        {/* The answers as the choice card's rows: one tone, allowing first and leading, refusing last. */}
        <VStack gap={2} className="decision-options">
            {options.map((o, i) => (
              <button
                key={o.optionId}
                className={answering === null || answering === o.optionId ? 'decision-option' : 'decision-option dimmed'}
                disabled={answering !== null}
                onClick={() => respond(o.optionId)}
              >
                <OptionBadge>{answering === o.optionId ? <Spinner size={12} /> : i + 1}</OptionBadge>
                <Text size="body" weight={i === 0 ? 'medium' : 'regular'} tone="text" className="decision-option-label">{optionLabel(o)}</Text>
              </button>
            ))}
        </VStack>
      </VStack>
    </div>
  )
}

function outcome(entry: Entry) {
  const d = entry.data
  switch (d.status) {
    case 'answered': {
      const chosen = d.options?.find((o) => o.optionId === d.selected)
      switch (chosen?.kind) {
        case 'allow_once':
          return trNow('Allowed once')
        case 'allow_always':
          return trNow('Always allowed')
        case 'reject_once':
          return trNow('Denied')
        case 'reject_always':
          return trNow('Never allowed')
        default:
          return chosen?.name ?? trNow('Answered')
      }
    }
    case 'cancelled':
      return trNow('Cancelled')
    default:
      return trNow('Expired — the agent moved on')
  }
}

export function CodeBox({ text }: { text: string }) {
  return (
    <div className="code-box">
      <pre className="selectable code-box-text">{text}</pre>
    </div>
  )
}

export function DiffView({ diff }: { diff: FileDiff }) {
  const lines = diff.patch.split('\n').slice(0, 80)
  return (
    <VStack gap={4}>
      <HStack gap={6} className="diff-header">
        <Icon name={diff.isNew ? 'doc.badge.plus' : 'doc.text'} size={10} />
        <Text truncate grow>{diff.path.split('/').pop()}</Text>
        <Text tone="added">+{diff.added}</Text>
        <Text tone="removed">−{diff.removed}</Text>
      </HStack>
      <div className="code-box diff-box">
        <div className="code-box-text">
          {lines.map((line, i) => (
            <Text as="div" key={i} tone={line.startsWith('+') ? 'added' : line.startsWith('-') ? 'removed' : 'secondary'} className="diff-line">
              {line || ' '}
            </Text>
          ))}
        </div>
      </div>
    </VStack>
  )
}

/** A settled request as one quiet line: what kind it is, its name (and where it comes from), how it ended. */
function RequestLine({ icon, title, end, expanded, onClick }: { icon: string; title: string; end: string; expanded: boolean; onClick: () => void }) {
  const { name, source, literal } = requestName(title)
  return (
    <button className="request-line" aria-expanded={expanded} onClick={onClick}>
      <Icon name={icon} size={12} color="var(--secondary)" />
      <Text size="subheadline" mono={literal} tone="text" truncate>{name}</Text>
      {source ? <Text size="subheadline" tone="tertiary" truncate>{source}</Text> : null}
      <Text size="caption" tone="tertiary" className="request-line-end">{end}</Text>
      <span className={`disclosure-chevron${expanded ? ' open' : ''}`}><Icon name="chevron.right" size={8} weight="semibold" /></span>
    </button>
  )
}

/**
 * Approvals in a row that ended alike, as one line: how many, where they come from and how they ended, with each one's
 * own line under it on demand (`row` draws it, the chat's live card).
 */
export function RequestRun({ entries, row }: { entries: Entry[]; row: (entry: Entry) => React.ReactNode }) {
  const tr = useTr()
  const [open, setOpen] = useState(false)
  const icons = new Set(entries.map((e) => requestIcon(e.data.toolKind, e.data.title)))
  const sources = [...new Set(entries.map((e) => requestName(e.data.title ?? '').source).filter((x) => x !== null))]
  return (
    <div className="agent-inset">
      <button className="request-line" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <Icon name={icons.size === 1 ? [...icons][0] : 'wrench.and.screwdriver'} size={12} color="var(--secondary)" />
        <Text size="subheadline" tone="text">{fill(tr('{count} tool requests'), { count: String(entries.length) })}</Text>
        {sources.length ? <Text size="subheadline" tone="tertiary" truncate>{sources.join(', ')}</Text> : null}
        <Text size="caption" tone="tertiary" className="request-line-end">{outcome(entries[0])}</Text>
        <span className={`disclosure-chevron${open ? ' open' : ''}`}><Icon name="chevron.right" size={8} weight="semibold" /></span>
      </button>
      <Reveal show={open}>
        <div className="request-run-list">{entries.map((e) => <div key={e.id}>{row(e)}</div>)}</div>
      </Reveal>
    </div>
  )
}

