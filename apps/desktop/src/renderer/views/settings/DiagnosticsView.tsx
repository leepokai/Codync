import { useEffect, useMemo, useState } from 'react'
import type { SentinelIssue, SentinelReport } from '@shared/models'
import { Button, CardForm, CardSection, Spinner, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { useApp } from '../../store/context'
import { MaintenanceStore } from '../../store/maintenance-store'
import { BotBundleSection } from './BotBundleSection'
import { BotCheckSection } from './BotCheckSection'
import { HealthSection } from './HealthSection'
import { TeamRunSection } from './TeamRunSection'
import { TokenBudgetSection } from './TokenBudgetSection'
import { errorText } from './parts'

const issueKinds: Record<SentinelIssue['kind'], string> = {
  tool_error: 'Tool error', retry_loop: 'Retry loop', turn_error: 'Failed turn', queue_lag: 'Queue wait',
}

function scanTime(timestamp: number): string | null {
  if (!Number.isFinite(timestamp)) return null
  const date = new Date(timestamp)
  return Number.isNaN(date.getTime()) ? null : date.toLocaleString()
}

/** Sentinel forensic diagnostics and health check (INIC-05). */
export function DiagnosticsView() {
  const app = useApp()
  const local = useModel(app.local)
  const running = local !== null && local.connection.kind === 'online'
  const [report, setReport] = useState<SentinelReport | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const maintenance = useModel(
    useMemo(() => new MaintenanceStore(async () => {
      if (!local) throw new Error('Start this computer’s host first.')
      return local.ready()
    }), [local]),
  )
  const bots = local ? [...local.bots.values()].filter((b) => b.kind !== 'group') : []
  const healthy = report?.issues.length === 0
  const scanned = report ? scanTime(report.scannedAt) : null

  const runScan = async () => {
    if (!local || !running) return
    setLoading(true)
    setError(null)
    try {
      const res = await local.sentinelHealth()
      setReport(res)
    } catch (e) {
      setError(errorText(e))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    if (running) void runScan()
  }, [running])

  return (
    <CardForm>
      <HealthSection />
      <CardSection
        title="Sentinel Watchdog"
        footer="Reviews recent tool errors, failed turns, retry loops, and queue waits."
      >
        <ValueRow
          label="Status"
          detail={
            report
              ? scanned ? `Last scanned ${scanned}` : 'Scan time unavailable'
              : running
                ? 'Ready to scan'
                : 'Host is offline'
          }
        >
          <HStack gap={8}>
            {report ? (
              <HStack gap={4}>
                <Icon
                  name={healthy ? 'checkmark.shield.fill' : 'exclamationmark.triangle.fill'}
                  size={13}
                  color={healthy ? 'var(--success, #30d158)' : 'var(--warning, #ff9f0a)'}
                />
                <Text tone={healthy ? 'text' : 'attention'}>
                  {healthy ? 'No issues found' : `${report.issues.length} ${report.issues.length === 1 ? 'issue' : 'issues'}`}
                </Text>
              </HStack>
            ) : null}
            <Button kind="secondary" disabled={!running || loading} onClick={() => void runScan()}>
              {loading ? <Spinner size={12} /> : null}
              {loading ? 'Scanning…' : 'Run diagnostic'}
            </Button>
          </HStack>
        </ValueRow>
        {error ? <Text size="footnote" tone="danger">{error}</Text> : null}
      </CardSection>

      {report ? (
        <CardSection title="Diagnostic Findings">
          <Text size="footnote" tone="secondary">{report.entriesExamined} entries examined.</Text>
          {report.issues.length === 0 ? (
            <HStack gap={8}>
              <Icon name="checkmark.circle.fill" size={14} color="var(--success, #30d158)" />
              <Text tone="secondary">No tool errors, failed turns, retry loops, or queue waits found.</Text>
            </HStack>
          ) : (
            report.issues.map((issue) => (
              <VStack key={`${issue.signature}:${issue.botId}`} gap={4} className="settings-row">
                <HStack gap={6}>
                  <Icon
                    name={issue.severity === 'high' ? 'xmark.octagon.fill' : issue.severity === 'medium' ? 'exclamationmark.triangle.fill' : 'info.circle'}
                    size={13}
                    color={issue.severity === 'high' ? 'var(--danger)' : issue.severity === 'medium' ? 'var(--warning)' : 'var(--secondary)'}
                  />
                  <Text weight="semibold">{issue.title}</Text>
                </HStack>
                <Text size="footnote" tone="secondary">
                  {issueKinds[issue.kind]} · {issue.severity} · {issue.count} {issue.count === 1 ? 'occurrence' : 'occurrences'}
                </Text>
                <Text size="footnote" tone="secondary">
                  Bot: {local?.bots.get(issue.botId)?.name ?? issue.botId}{issue.toolName ? ` · Tool: ${issue.toolName}` : ''}
                </Text>
                <Text size="caption" tone="tertiary">
                  {issue.wastedTurns} wasted turns · First seen: {scanTime(issue.firstSeenAt) ?? 'Unknown'} · Last seen: {scanTime(issue.lastSeenAt) ?? 'Unknown'}
                </Text>
                <Text size="caption" tone="tertiary">Evidence entry: {issue.evidenceEntryId}</Text>
              </VStack>
            ))
          )}
        </CardSection>
      ) : null}
      <BotCheckSection store={maintenance} online={running} />
      <TeamRunSection bots={bots} online={running} send={(text, botId) => local?.send(text, botId)} />
      <TokenBudgetSection store={maintenance} bots={bots} online={running} />
      <BotBundleSection store={maintenance} bots={bots} online={running} />
    </CardForm>
  )
}
