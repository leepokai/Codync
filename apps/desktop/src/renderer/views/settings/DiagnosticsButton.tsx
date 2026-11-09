import { useState } from 'react'
import { Button } from '../../components/Controls'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useApp } from '../../store/context'

/**
 * Copies a support snapshot: versions, health, counts, which channels are set up and the end of the host log,
 * with tokens and keys taken out by the host. Nothing leaves the computer until the person pastes it somewhere.
 */
export function DiagnosticsButton() {
  const app = useApp()
  const tr = useTr()
  const [state, setState] = useState<'idle' | 'busy' | 'copied' | 'error'>('idle')
  const copy = async () => {
    setState('busy')
    try {
      const local = app.local
      if (!local) throw new Error(tr('This computer’s host is not answering.'))
      const snapshot = await (await local.ready()).call<Record<string, unknown>>('diagnostics')
      window.codync.app.copy(JSON.stringify(snapshot, null, 2))
      setState('copied')
    } catch {
      setState('error')
    }
  }
  return (
    <HStack gap={8}>
      <Button kind="secondary" disabled={state === 'busy'} onClick={() => void copy()}>
        {tr(state === 'busy' ? 'Preparing…' : 'Copy diagnostics')}
      </Button>
      <Text size="caption" tone={state === 'error' ? 'danger' : 'tertiary'} grow>
        {state === 'copied'
          ? tr('Copied. Paste it where you need it; secrets were removed.')
          : state === 'error'
            ? tr('The snapshot could not be made.')
            : tr('Versions, health, settings and the end of the log, with secrets removed.')}
      </Text>
    </HStack>
  )
}
