import { useState } from 'react'
import type { Entry, FileDiff, PermissionOption } from '@shared/models'
import { Disclosure, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import './permission-card.css'
import { useTr } from '../../lib/use-language'


/** What the agent wants, from the tool's kind (also the notch island's headline). */
export const permissionHeadline = (toolKind?: string) => {
  switch (toolKind) {
    case 'execute':
      return 'Wants to run a command'
    case 'edit':
    case 'delete':
    case 'move':
      return 'Wants to change files'
    case 'fetch':
      return 'Wants to access the web'
    case 'read':
    case 'search':
      return 'Wants to read files'
    default:
      return 'Wants to use a tool'
  }
}

const rank: Record<string, number> = { allow_once: 0, allow_always: 1, reject_once: 2, reject_always: 3 }

/** Allow before deny, once before always. */
export const sortedOptions = (options: PermissionOption[]) => [...options].sort((a, b) => (rank[a.kind] ?? 9) - (rank[b.kind] ?? 9))

export function optionLabel(o: PermissionOption) {
  switch (o.kind) {
    case 'allow_once':
      return 'Allow once'
    case 'allow_always':
      return 'Always allow'
    case 'reject_once':
      return 'Deny'
    case 'reject_always':
      return 'Never'
    default:
      return o.name
  }
}

/** Approval card in Grok Bot's choice-card style: what the agent wants, where it runs, the answers. */
export function PermissionCard({ entry, hostName, answering, respond }: { entry: Entry; hostName: string; answering: string | null; respond: (option: string | null) => void }) {
  const tr = useTr()
  const [expanded, setExpanded] = useState(false)
  const d = entry.data
  const pending = d.status === 'pending'
  const hasDetail = !!d.command || !!d.detail || !!d.diffs?.length
  const options = sortedOptions(d.options ?? [])
  const cwd = d.cwd ? ` · ${d.cwd.split('/').filter(Boolean).pop() ?? d.cwd}` : ''
  return (
    <div className="agent-inset">
      <VStack gap={10} className="permission-card">
        <HStack gap={8}>
          <Text size="headline" tone="text">{permissionHeadline(d.toolKind)}</Text>
          {pending ? <span className="permission-card-dot" /> : null}
        </HStack>
        <Text as="div" size="subheadline" mono tone="secondary" lines={expanded ? undefined : 3} className="selectable permission-card-title">
          {d.title ?? ''}
        </Text>
        <HStack gap={6}>
          <Icon name="desktopcomputer" size={10} color="var(--tertiary)" />
          <Text size="caption" tone="tertiary">{`Runs on ${hostName}${cwd}`}</Text>
        </HStack>
        {hasDetail ? (
          <Disclosure expanded={expanded} onToggle={() => setExpanded((e) => !e)} label={<Text size="caption" weight="medium" tone="secondary">{tr(`Details`)}</Text>}>
            <VStack gap={8} className="permission-card-details">
              {d.command ? <CodeBox text={d.command} /> : null}
              {d.detail ? <CodeBox text={d.detail.slice(0, 2000)} /> : null}
              {(d.diffs ?? []).map((diff, i) => <DiffView key={i} diff={diff} />)}
            </VStack>
          </Disclosure>
        ) : null}
        {pending ? (
          <div className="permission-card-options">
            {options.map((o, i) => (
              <div key={o.optionId}>
                {i > 0 ? <div className="thread-hairline" /> : null}
                <button
                  className={answering === null || answering === o.optionId ? 'permission-option' : 'permission-option dimmed'}
                  disabled={answering !== null}
                  onClick={() => respond(o.optionId)}
                >
                  <Text size="body" weight={o.kind === 'allow_once' ? 'semibold' : 'regular'} tone={o.kind.startsWith('allow') ? 'text' : 'danger'} className="permission-option-label">{optionLabel(o)}</Text>
                  {answering === o.optionId ? <Spinner size={14} /> : null}
                </button>
              </div>
            ))}
          </div>
        ) : (
          <Text as="div" size="caption" weight="medium" tone="secondary">{outcome(entry)}</Text>
        )}
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
          return 'Allowed once'
        case 'allow_always':
          return 'Always allowed'
        case 'reject_once':
          return 'Denied'
        case 'reject_always':
          return 'Never allowed'
        default:
          return chosen?.name ?? 'Answered'
      }
    }
    case 'cancelled':
      return 'Cancelled'
    default:
      return 'Expired — the agent moved on'
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
