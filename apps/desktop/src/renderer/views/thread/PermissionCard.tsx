import { useState } from 'react'
import type { Entry, FileDiff, PermissionOption } from '@shared/models'
import { Disclosure, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { font, px } from '../../lib/fonts'

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
  const [expanded, setExpanded] = useState(false)
  const d = entry.data
  const pending = d.status === 'pending'
  const hasDetail = !!d.command || !!d.detail || !!d.diffs?.length
  const options = sortedOptions(d.options ?? [])
  const cwd = d.cwd ? ` · ${d.cwd.split('/').filter(Boolean).pop() ?? d.cwd}` : ''
  return (
    <div style={{ paddingRight: 40 }}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 10, padding: 16, background: 'var(--bubble-agent)', borderRadius: 22 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style={{ ...font('headline'), color: 'var(--text)' }}>{permissionHeadline(d.toolKind)}</span>
          {pending ? <span style={{ width: 7, height: 7, borderRadius: '50%', background: 'var(--warning)' }} /> : null}
        </div>
        <div
          className="selectable"
          style={{
            ...font('subheadline', undefined, 'monospaced'), color: 'var(--secondary)', overflowWrap: 'anywhere',
            ...(expanded ? {} : { display: '-webkit-box', WebkitLineClamp: 3, WebkitBoxOrient: 'vertical', overflow: 'hidden' }),
          }}
        >
          {d.title ?? ''}
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, ...font('caption'), color: 'var(--tertiary)' }}>
          <Icon name="desktopcomputer" size={10} />
          {`Runs on ${hostName}${cwd}`}
        </div>
        {hasDetail ? (
          <Disclosure expanded={expanded} onToggle={() => setExpanded((e) => !e)} label={<span style={{ ...font('caption', 'medium'), color: 'var(--secondary)' }}>Details</span>}>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8, paddingTop: 6 }}>
              {d.command ? <CodeBox text={d.command} /> : null}
              {d.detail ? <CodeBox text={d.detail.slice(0, 2000)} /> : null}
              {(d.diffs ?? []).map((diff, i) => <DiffView key={i} diff={diff} />)}
            </div>
          </Disclosure>
        ) : null}
        {pending ? (
          <div style={{ background: 'var(--background)', borderRadius: 14, marginTop: 4 }}>
            {options.map((o, i) => (
              <div key={o.optionId}>
                {i > 0 ? <div style={{ height: 0.5, background: 'var(--border)' }} /> : null}
                <button
                  disabled={answering !== null}
                  onClick={() => respond(o.optionId)}
                  style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%', minHeight: 46, padding: '0 14px', opacity: answering === null || answering === o.optionId ? 1 : 0.4 }}
                >
                  <span style={{ ...font('body', o.kind === 'allow_once' ? 'semibold' : 'regular'), color: o.kind.startsWith('allow') ? 'var(--text)' : 'var(--danger)', flex: 1 }}>{optionLabel(o)}</span>
                  {answering === o.optionId ? <Spinner size={14} /> : null}
                </button>
              </div>
            ))}
          </div>
        ) : (
          <div style={{ ...font('caption', 'medium'), color: 'var(--secondary)' }}>{outcome(entry)}</div>
        )}
      </div>
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
    <div style={{ background: 'var(--code-background)', borderRadius: 8, overflow: 'auto', maxHeight: 240 }}>
      <pre className="selectable" style={{ margin: 0, padding: 8, fontFamily: 'var(--mono)', fontSize: px(10), color: 'var(--text)', width: 'max-content' }}>{text}</pre>
    </div>
  )
}

export function DiffView({ diff }: { diff: FileDiff }) {
  const lines = diff.patch.split('\n').slice(0, 80)
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 6, ...font('caption', undefined, 'monospaced'), color: 'var(--secondary)' }}>
        <Icon name={diff.isNew ? 'doc.badge.plus' : 'doc.text'} size={10} />
        <span style={{ flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{diff.path.split('/').pop()}</span>
        <span style={{ color: 'var(--added)' }}>+{diff.added}</span>
        <span style={{ color: 'var(--removed)' }}>−{diff.removed}</span>
      </div>
      <div style={{ background: 'var(--code-background)', borderRadius: 8, overflow: 'auto', maxHeight: 260 }}>
        <div style={{ padding: 8, fontFamily: 'var(--mono)', fontSize: px(10), width: 'max-content' }}>
          {lines.map((line, i) => (
            <div key={i} style={{ whiteSpace: 'pre', color: line.startsWith('+') ? 'var(--added)' : line.startsWith('-') ? 'var(--removed)' : 'var(--secondary)' }}>
              {line || ' '}
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
