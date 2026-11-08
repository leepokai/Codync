import { useEffect, useState } from 'react'
import { Button, CardSection } from '../../components/Controls'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useApp } from '../../store/context'

interface AuditLine {
  at: number
  action: string
  detail: string
}

/** The security decisions made on this computer, newest first; kept for 90 days. */
export function AuditLogSection() {
  const tr = useTr()
  const app = useApp()
  const [lines, setLines] = useState<AuditLine[]>([])
  const [error, setError] = useState<string | null>(null)
  const read = async () => {
    const local = app.local
    if (!local) return
    try {
      setLines((await (await local.ready()).call<{ lines: AuditLine[] }>('auditLog')).lines)
      setError(null)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }
  useEffect(() => { void read() }, [app])
  return (
    <CardSection title={tr('Audit log')} footer={tr('Pairing, revoked access, cloud, network and approval decisions. Nothing from chats; kept for 90 days.')}>
      <div className="settings-row">
        <Button kind="secondary" onClick={() => void read()}>{tr('Refresh')}</Button>
        <Button kind="secondary" disabled={!lines.length} onClick={async () => {
          const local = app.local
          if (!local) return
          await (await local.ready()).call('clearAuditLog')
          setLines([])
        }}>{tr('Clear log')}</Button>
      </div>
      {error ? <Text role="alert" tone="danger">{error}</Text> : null}
      {!lines.length ? <Text tone="secondary">{tr('Nothing yet.')}</Text> : null}
      {lines.map((line, index) => (
        <div key={`${line.at}-${index}`} className="settings-row">
          <Text grow>{tr(line.action)}</Text>
          <Text size="caption" tone="tertiary">{line.detail || '—'}</Text>
          <Text size="caption" tone="tertiary">{new Date(line.at).toLocaleString()}</Text>
        </div>
      ))}
    </CardSection>
  )
}
