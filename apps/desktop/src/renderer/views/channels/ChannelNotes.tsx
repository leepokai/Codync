import { useEffect, useState } from 'react'
import { Button, IconButton } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import type { ChannelsStore } from '../../store/channels-store'

/** Small facts this channel keeps, given to the bot with its memory before every message. */
export function ChannelNotes({ provider, channel, store }: {
  provider: 'telegram' | 'slack' | 'discord'
  channel: string
  store: ChannelsStore
}) {
  const tr = useTr()
  const [notes, setNotes] = useState<string[]>([])
  const [text, setText] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let live = true
    void store.notes(provider, channel).then((list) => {
      if (live) setNotes(list)
    }, () => {})
    return () => {
      live = false
    }
  }, [provider, channel, store])
  const run = async (work: () => Promise<string[]>) => {
    setBusy(true)
    try {
      setNotes(await work())
      setError(null)
      return true
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
      return false
    } finally {
      setBusy(false)
    }
  }
  const add = async () => {
    if (!text.trim()) return
    if (await run(() => store.addNote(provider, channel, text.trim()))) setText('')
  }
  return (
    <VStack gap={4}>
      <Text size="caption" tone="tertiary">{tr('Notes the bot gets with this channel’s memory')}</Text>
      {notes.map((note, index) => (
        <HStack key={index} gap={8}>
          <Text size="caption" grow>{note}</Text>
          <IconButton title={tr('Remove note')} icon="minus.circle" size={14} disabled={busy}
            onClick={() => void run(() => store.removeNote(provider, channel, index))} />
        </HStack>
      ))}
      <HStack gap={6}>
        <input className="field-box" value={text} maxLength={300} spellCheck={false}
          placeholder={tr('A fact this channel should keep')} aria-label={tr('Channel note')}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') void add() }} />
        <Button kind="secondary" disabled={busy || !text.trim()} onClick={() => void add()}>{tr('Add')}</Button>
      </HStack>
      {error ? <Text size="caption" role="alert" tone="danger">{error}</Text> : null}
    </VStack>
  )
}
