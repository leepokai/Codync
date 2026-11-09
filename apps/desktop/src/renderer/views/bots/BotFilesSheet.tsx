import { useEffect, useState } from 'react'
import type { Bot } from '@shared/models'
import type { BotStore } from '../../store/bot-store'
import { Button, IconButton } from '../../components/Controls'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'

interface Row {
  name: string
  dir: boolean
  size: number
  modified: number | null
}

const kb = (bytes: number) => (bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024 ? `${Math.round(bytes / 1024)} KB` : `${(bytes / (1024 * 1024)).toFixed(1)} MB`)

/** A bot's folder, one level at a time: open folders, copy a file's path. The host keeps it inside the workspace. */
export function BotFilesSheet({ bot, store, onClose }: { bot: Bot; store: BotStore; onClose: () => void }) {
  const tr = useTr()
  const [path, setPath] = useState('')
  const [rows, setRows] = useState<Row[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const load = async (next: string) => {
    try {
      const client = await store.ready()
      const answer = await client.call<{ entries: Row[] }>('listFiles', { botId: bot.id, path: next })
      setRows(answer.entries)
      setPath(next)
      setError(null)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }
  useEffect(() => {
    void load('')
  }, [bot.id])
  const segments = path.split('/').filter(Boolean)
  const up = () => void load(segments.slice(0, -1).join('/'))
  const full = (name: string) => [bot.cwd.replace(/[\\/]+$/, ''), ...segments, name].filter(Boolean).join('/')
  return (
    <SheetPage>
      <ModalHeader title={tr('Files')} subtitle={tr('Inside this bot’s folder')} onClose={onClose} closeTitle={tr('Close')} />
      <VStack gap={10} className="channel-sheet">
        <HStack gap={6} wrap>
          <Button kind="secondary" disabled={!segments.length} onClick={up}>{tr('Up')}</Button>
          <Text size="caption" tone="secondary">{`${bot.name}${segments.length ? `/${segments.join('/')}` : ''}`}</Text>
        </HStack>
        {rows === null && !error ? <Text tone="secondary">{tr('Loading…')}</Text> : null}
        {rows?.length === 0 ? <Text size="caption" tone="tertiary">{tr('This folder is empty.')}</Text> : null}
        {rows?.map((row) => (
          <HStack key={`${row.dir ? 'd' : 'f'}:${row.name}`} gap={8}>
            <IconButton title={row.dir ? tr('Open folder') : tr('Copy path')} icon={row.dir ? 'folder' : 'doc'}
              size={16} onClick={() => row.dir ? void load([...segments, row.name].join('/')) : window.codync.app.copy(full(row.name))} />
            <Text size="caption" grow>{row.name}</Text>
            <Text size="caption" tone="tertiary">{row.dir ? tr('Folder') : kb(row.size)}</Text>
          </HStack>
        ))}
        {error ? <Text size="caption" role="alert" tone="danger">{error}</Text> : null}
      </VStack>
    </SheetPage>
  )
}
