import { useState } from 'react'
import type { Entry, EntryData, FileDiff, PermissionOption } from '@shared/models'
import { Reveal, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import './permission-card.css'
import { useTr } from '../../lib/use-language'
import { trNow } from '../../lib/l10n.ts'
import { fill } from '@shared/fill'
import { permissionOptionLabel, permissionOutcomeLabel, requestDetail, requestIcon, requestName, requestParams } from './requests'
import { OptionBadge } from './DecisionCard'


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

export function optionLabel(o: PermissionOption, options?: PermissionOption[]) {
  return permissionOptionLabel(o, options, trNow)
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
export function PermissionCard({ entry, hostName, answering, respond, result }: {
  entry: Entry
  hostName: string
  answering: string | null
  respond: (option: string | null) => void
  /** Once allowed: the tool entry of the call it asked for, with what it returned. */
  result?: EntryData
}) {
  const tr = useTr()
  const [expanded, setExpanded] = useState(false)
  const [open, setOpen] = useState(false)
  const d = entry.data
  const pending = d.status === 'pending'
  const detail = requestDetail(d.detail)
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
            {requestParams(detail) ? <ParamRows params={requestParams(detail) ?? []} clamp={false} /> : detail ? <CodeBox text={detail.slice(0, 2000)} /> : null}
            {(d.diffs ?? []).map((diff, i) => <DiffView key={i} diff={diff} />)}
            {result?.output || result?.diffs?.length ? (
              <VStack gap={6}>
                <Text size="caption" weight="medium" tone="secondary">{tr('Output')}</Text>
                {(result.diffs ?? []).map((diff, i) => <DiffView key={i} diff={diff} />)}
                {result.output ? <CodeBox text={result.output.slice(0, 4000)} /> : null}
              </VStack>
            ) : null}
          </VStack>
        </Reveal>
      </div>
    )
  }
  const { name, source, literal } = requestName(d.title ?? '')
  const params = requestParams(detail)
  const shownParams = expanded ? params : params?.slice(0, 4)
  // Rows past the first four: counted on Details while folded, and Details stays so they fold back.
  const extraParams = Math.max(0, (params?.length ?? 0) - 4)
  const hiddenParams = expanded ? 0 : extraParams
  // Anything params rows don't already show sits behind Details.
  const command = d.command && d.command !== name ? d.command : null
  const hasMore = !!command || (!params && !!detail) || !!d.diffs?.length || extraParams > 0
  return (
    <div className="agent-inset">
      <VStack gap={12} className="permission-card approval-card">
        <VStack gap={8}>
          {/* What asks (its kind, or the server a tool comes from), then the request itself, then where it runs. */}
          <HStack gap={6}>
            <Icon name={requestIcon(d.toolKind, d.title)} size={12} color="var(--secondary)" />
            <Text size="caption" tone="secondary" truncate grow>{source ?? permissionHeadline(d.toolKind)}</Text>
          </HStack>
          <VStack gap={2}>
            {literal ? (
              <Text as="div" size="subheadline" mono tone="secondary" lines={expanded ? undefined : 3} className="selectable permission-card-title">{name}</Text>
            ) : (
              <Text as="div" size="body" weight="medium" tone="text" className="permission-card-title">{name}</Text>
            )}
            <HStack gap={8} className="approval-card-meta">
              <Text size="caption" tone="tertiary" truncate grow>{where}</Text>
              {hasMore ? (
                <button className="approval-card-details" aria-expanded={expanded} onClick={() => setExpanded((e) => !e)}>
                  <Text size="caption" weight="medium" tone="secondary">{hiddenParams > 0 ? `${tr('Details')} · ${hiddenParams}` : tr('Details')}</Text>
                  <span className={`disclosure-chevron${expanded ? ' open' : ''}`}><Icon name="chevron.right" size={8} weight="semibold" /></span>
                </button>
              ) : null}
            </HStack>
            <ThreadLine entry={entry} />
          </VStack>
          {shownParams?.length ? <ParamRows params={shownParams} clamp={!expanded} /> : null}
        </VStack>
        <Reveal show={expanded && (!!command || (!params && !!detail) || !!d.diffs?.length)}>
          <VStack gap={8}>
            {command ? <CodeBox text={command} /> : null}
            {!params && detail ? <CodeBox text={detail.slice(0, 2000)} /> : null}
            {(d.diffs ?? []).map((diff, i) => <DiffView key={i} diff={diff} />)}
          </VStack>
        </Reveal>
        {/* The answers as the choice card's numbered rows: allow before deny, once before always. */}
        <VStack gap={2} className="decision-options">
          {options.map((o, i) => (
            <button
              key={o.optionId}
              className={answering === null || answering === o.optionId ? 'decision-option approval-answer' : 'decision-option approval-answer dimmed'}
              disabled={answering !== null}
              onClick={() => respond(o.optionId)}
            >
              <OptionBadge>{answering === o.optionId ? <Spinner size={12} /> : i + 1}</OptionBadge>
              <Text size="body" tone="text" className="decision-option-label">{optionLabel(o, options)}</Text>
            </button>
          ))}
        </VStack>
      </VStack>
    </div>
  )
}

/** Arguments as a quiet two-column list: the name, then its value (clamped to a few lines until opened). */
function ParamRows({ params, clamp }: { params: [string, string][]; clamp: boolean }) {
  return (
    <dl className="param-rows">
      {params.map(([k, v]) => (
        <div key={k} className="param-row">
          <Text as="div" size="caption" tone="tertiary" truncate className="param-row-name">{k}</Text>
          <Text as="div" size="caption" mono tone="secondary" lines={clamp ? 3 : undefined} className="selectable param-row-value">{v}</Text>
        </div>
      ))}
    </dl>
  )
}

function outcome(entry: Entry) {
  const d = entry.data
  return permissionOutcomeLabel(d.status, d.selected ?? undefined, d.options, trNow)
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

/** A settled request as one quiet line: what kind it is, what it did (where it comes from in its tip), how it ended. */
function RequestLine({ icon, title, end, expanded, onClick }: { icon: string; title: string; end: string; expanded: boolean; onClick: () => void }) {
  const { name, source, literal } = requestName(title)
  return (
    <button className="request-line" aria-expanded={expanded} title={source ?? undefined} onClick={onClick}>
      <Icon name={icon} size={12} color="var(--secondary)" />
      <Text size="subheadline" mono={literal} tone="secondary" truncate>{name}</Text>
      <Text size="caption" tone="tertiary" className="request-line-end">{end}</Text>
      <span className={`disclosure-chevron${expanded ? ' open' : ''}`}><Icon name="chevron.right" size={8} weight="semibold" /></span>
    </button>
  )
}

/**
 * Approvals in a row that ended alike, as one line: what they did, in a sentence of their names, how many and how they
 * ended, with each one's own line under it on demand (`row` draws it, the chat's live card).
 */
export function RequestRun({ entries, row }: { entries: Entry[]; row: (entry: Entry) => React.ReactNode }) {
  const tr = useTr()
  const [open, setOpen] = useState(false)
  const icons = new Set(entries.map((e) => requestIcon(e.data.toolKind, e.data.title)))
  const names = [...new Set(entries.map((e) => requestName(e.data.title ?? '').name))]
  return (
    <div className="agent-inset">
      <button className="request-line" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <Icon name={icons.size === 1 ? [...icons][0] : 'terminal'} size={12} color="var(--secondary)" />
        <Text size="subheadline" tone="secondary" truncate title={fill(tr('{count} tool requests'), { count: String(entries.length) })}>{names.join(', ')}</Text>
        <Text size="subheadline" tone="tertiary">{`· ${entries.length}`}</Text>
        <Text size="caption" tone="tertiary" className="request-line-end">{outcome(entries[0])}</Text>
        <span className={`disclosure-chevron${open ? ' open' : ''}`}><Icon name="chevron.right" size={8} weight="semibold" /></span>
      </button>
      <Reveal show={open}>
        <div className="request-run-list">{entries.map((e) => <div key={e.id}>{row(e)}</div>)}</div>
      </Reveal>
    </div>
  )
}

