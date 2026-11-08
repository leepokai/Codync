import { useEffect, useState } from 'react'
import { Button } from '../../components/Controls'
import { VStack } from '../../components/Stack'
import { useTr } from '../../lib/use-language'
import { Field } from '../marketplace/pieces'

/**
 * What the bot should know about one channel, given before every message from it. Saved on its
 * own so the channel's connection settings are not resent.
 */
export function ChannelMemory({ label, value, save }: { label?: string; value: string; save: (memory: string) => Promise<boolean> }) {
  const tr = useTr()
  const [text, setText] = useState(value)
  const [busy, setBusy] = useState(false)
  useEffect(() => setText(value), [value])
  const dirty = text.trim() !== value.trim()
  return (
    <VStack gap={4} className="settings-stack">
      <Field label={label ?? tr('Channel memory')} value={text} onChange={setText} lines={[2, 4]}
        placeholder={tr('What the bot should remember about this channel')} />
      {dirty ? (
        <div>
          <Button kind="secondary" disabled={busy} onClick={() => { setBusy(true); void save(text.trim()).finally(() => setBusy(false)) }}>
            {tr('Save memory')}
          </Button>
        </div>
      ) : null}
    </VStack>
  )
}
