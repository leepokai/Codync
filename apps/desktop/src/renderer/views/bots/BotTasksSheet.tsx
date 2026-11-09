import { useEffect, useState } from 'react'
import type { Bot } from '@shared/models'
import type { BotStore } from '../../store/bot-store'
import { Button } from '../../components/Controls'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'

interface Approval {
  id: string
  title: string
  detail: string
  since: number
}

interface Run {
  id: string
  routineId: string
  status: string
  startedAt: number | null
  finishedAt: number | null
  detail: string | null
  threadRoot: string | null
}

interface Thread {
  id: string
  unread: number
}

export interface BotTasks {
  approvals: Approval[]
  runs: Run[]
  threads: Thread[]
}

const RUN_STATUS: Record<string, string> = {
  pending: 'Waiting its turn',
  starting: 'Starting',
  running: 'Running',
  recovering: 'Resuming after a restart',
  succeeded: 'Done',
  failed: 'Failed',
  interrupted: 'Interrupted',
  cancelled: 'Cancelled',
}

const ACTIVE = new Set(['pending', 'starting', 'running', 'recovering'])

const threadName = (id: string) =>
  id.startsWith('doc:') ? 'Document' : id.startsWith('doc-group:') ? 'Group document' : id.startsWith('slack-') ? 'Slack thread' : id.startsWith('routine') ? 'Routine thread' : id

/**
 * What a bot waits on: approvals left open, routine runs (active and recent) and side conversations with
 * unread replies. Derived from the host on each open; nothing here changes the bot.
 */
export function BotTasksSheet({ bot, store, onClose }: { bot: Bot; store: BotStore; onClose: () => void }) {
  const tr = useTr()
  const [tasks, setTasks] = useState<BotTasks | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const load = async () => {
    setBusy(true)
    try {
      setTasks(await store.botTasks(bot.id))
      setError(null)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
    setBusy(false)
  }
  useEffect(() => {
    void load()
    const timer = setInterval(() => void load(), 8_000)
    return () => clearInterval(timer)
  }, [bot.id])
  const runs = (tasks?.runs ?? []).slice(0, 8)
  return (
    <SheetPage>
        <ModalHeader title={tr('Tasks')} subtitle={bot.name} onClose={onClose} closeTitle={tr('Close')} />
        <VStack gap={14} className="channel-sheet">
          <section className="channel-item">
            <HStack gap={8}>
              <Text weight="semibold" grow>{tr('Approvals')}</Text>
              <Text size="caption" tone="secondary">{String(tasks?.approvals.length ?? 0)}</Text>
            </HStack>
            {tasks?.approvals.length ? tasks.approvals.map((approval) => (
              <VStack key={approval.id} gap={2}>
                <Text size="caption">{approval.title}</Text>
                {approval.detail ? <Text size="caption" tone="tertiary" className="bot-task-detail">{approval.detail}</Text> : null}
              </VStack>
            )) : <Text size="caption" tone="tertiary">{tr('Nothing waiting for you.')}</Text>}
          </section>

          <section className="channel-item">
            <HStack gap={8}>
              <Text weight="semibold" grow>{tr('Routine runs')}</Text>
              <Text size="caption" tone="secondary">{String((tasks?.runs ?? []).filter((run) => ACTIVE.has(run.status)).length)} {tr('active')}</Text>
            </HStack>
            {runs.length ? runs.map((run) => (
              <HStack key={run.id} gap={8}>
                <Text size="caption" grow>{tr(RUN_STATUS[run.status] ?? run.status)}</Text>
                <Text size="caption" tone="tertiary">{run.threadRoot ? tr('In a thread') : tr('In the chat')}</Text>
              </HStack>
            )) : <Text size="caption" tone="tertiary">{tr('No routine runs yet.')}</Text>}
          </section>

          <section className="channel-item">
            <HStack gap={8}>
              <Text weight="semibold" grow>{tr('Unread threads')}</Text>
              <Text size="caption" tone="secondary">{String(tasks?.threads.length ?? 0)}</Text>
            </HStack>
            {tasks?.threads.length ? tasks.threads.map((thread) => (
              <HStack key={thread.id} gap={8}>
                <Text size="caption" grow>{tr(threadName(thread.id))}</Text>
                <Text size="caption" tone="attention">{String(thread.unread)}</Text>
              </HStack>
            )) : <Text size="caption" tone="tertiary">{tr('Every thread is read.')}</Text>}
          </section>

          <HStack gap={8}>
            <Button kind="secondary" disabled={busy} onClick={() => void load()}>{tr('Refresh')}</Button>
            <Button kind="secondary" onClick={onClose}>{tr('Done')}</Button>
          </HStack>
          {error ? <Text size="caption" role="alert" tone="danger">{error}</Text> : null}
        </VStack>
    </SheetPage>
  )
}
