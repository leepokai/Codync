import { useState } from 'react'
import { isWorkingIn, type Entry } from '@shared/models'
import { Icon } from '../../components/Icon'
import { Button } from '../../components/Controls'
import { ModalHeader, Sheet, usePresence } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { previewOf } from './queued-preview'
import './queued.css'

/**
 * The messages waiting behind the running turn, as iOS `QueuedMessages` and the Android `QueuedStrip`: the next one
 * over the message box with Edit, Steer and Delete, and the whole queue in a sheet. Steer hands one to the agent now
 * (host `steer`) instead of after the turn; Edit and Delete only change what waits.
 */
export function QueuedStrip({ botId, thread = null }: { botId: string; thread?: string | null }) {
  const store = useStore()
  const tr = useTr()
  const [open, setOpen] = useState(false)
  const [editing, setEditing] = useState<Entry | null>(null)
  const [draft, setDraft] = useState('')
  const bot = store.bots.get(botId)
  const queued = store
    .chatEntries(botId)
    .filter((e) => (e.threadId ?? null) === thread && e.kind === 'user' && e.data.status === 'queued')
  const working = !!bot && isWorkingIn(bot, botId)
  const [next, ...rest] = queued
  const reveal = usePresence(!!next)

  const edit = (entry: Entry) => {
    setDraft(entry.data.text ?? '')
    setEditing(entry)
    setOpen(true)
  }
  const save = async () => {
    const entry = editing
    const text = draft.trim()
    if (!entry || !text) return
    const error = await store.editQueued(entry, text)
    if (error) return store.setError(error)
    setEditing(null)
  }
  const steer = async (entry: Entry) => {
    const error = await store.steer(entry)
    if (error) store.setError(error)
    else setOpen(false)
  }
  const drop = async (entry: Entry) => {
    const error = await store.deleteQueued(entry)
    if (error) store.setError(error)
  }

  if (!reveal.mounted) return null

  return (
    <>
      <div className={`composer-reveal queued-reveal ${reveal.shown ? 'shown' : ''}`}>
        <div>
          {next ? (
            <div className="queued-strip">
              <VStack gap={2} className="queued-card">
                <VStack gap={1} className="queued-next">
                  <Text size="caption2" tone="tertiary">{tr('Waiting to be sent')}</Text>
                  <Text size="subheadline" tone="text" lines={2}>{previewOf(next)}</Text>
                </VStack>
                <HStack gap={2} className="queued-actions">
                  <button className="press queued-action" onClick={() => edit(next)}>
                    <Icon name="pencil" size={11} weight="semibold" color="var(--secondary)" />
                    {tr('Edit')}
                  </button>
                  {working ? (
                    <button className="press queued-action accent" onClick={() => void steer(next)}>
                      <Icon name="arrow.turn.down.right" size={11} weight="semibold" color="var(--accent)" />
                      {tr('Steer it into the reply now')}
                    </button>
                  ) : null}
                  <button className="press queued-action" onClick={() => void drop(next)}>
                    <Icon name="trash" size={11} weight="semibold" color="var(--secondary)" />
                    {tr('Delete')}
                  </button>
                </HStack>
                {rest.length ? (
                  <button className="press queued-more" onClick={() => setOpen(true)}>
                    <Icon name="rectangle.stack.fill" size={11} weight="semibold" color="var(--secondary)" />
                    <Text size="caption" tone="secondary">{rest.length === 1 ? tr('1 more waiting') : tr('{count} more waiting').replace('{count}', `${rest.length}`)}</Text>
                    <Icon name="chevron.right" size={9} weight="semibold" color="var(--tertiary)" />
                  </button>
                ) : null}
              </VStack>
            </div>
          ) : null}
        </div>
      </div>
      <Sheet open={open} onClose={() => { setOpen(false); setEditing(null) }} width="min(420px, 92vw)" maxHeight="70vh">
        {editing ? (
          <VStack className="sheet-page">
            <ModalHeader title={tr('Edit message')} onClose={() => setEditing(null)} />
            <div className="queued-editor">
              <textarea
                className="queued-editor-field"
                value={draft}
                aria-label={tr('Message')}
                rows={4}
                autoFocus
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault()
                    void save()
                  }
                }}
              />
              <HStack gap={8} justify="end">
                <Button kind="secondary" onClick={() => setEditing(null)}>{tr('Cancel')}</Button>
                <Button kind="secondary" disabled={!draft.trim() || draft.trim() === editing.data.text} onClick={() => void save()}>{tr('Save')}</Button>
              </HStack>
            </div>
          </VStack>
        ) : (
          <VStack className="sheet-page">
            <ModalHeader title={tr('Message queue')} subtitle={tr('Steer sends a message into the reply now; Edit and Delete only change what waits.')} />
            {queued.map((entry) => (
              <VStack key={entry.id} gap={4} className="queued-row">
                <Text size="body" tone="text" lines={4}>{previewOf(entry)}</Text>
                <HStack gap={2}>
                  <button className="press queued-action" onClick={() => edit(entry)}>
                    <Icon name="pencil" size={11} weight="semibold" color="var(--secondary)" />
                    {tr('Edit')}
                  </button>
                  {working ? (
                    <button className="press queued-action accent" onClick={() => void steer(entry)}>
                      <Icon name="arrow.turn.down.right" size={11} weight="semibold" color="var(--accent)" />
                      {tr('Steer it into the reply now')}
                    </button>
                  ) : null}
                  <button className="press queued-action" onClick={() => void drop(entry)}>
                    <Icon name="trash" size={11} weight="semibold" color="var(--secondary)" />
                    {tr('Delete')}
                  </button>
                </HStack>
              </VStack>
            ))}
          </VStack>
        )}
      </Sheet>
    </>
  )
}
