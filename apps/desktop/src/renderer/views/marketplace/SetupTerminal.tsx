import { useRef } from 'react'
import { Button } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { SheetPage } from '../../components/Overlay'
import { HStack, Spacer } from '../../components/Stack'
import { Text } from '../../components/Text'
import { ThinkingOrb } from '../../components/ThinkingOrb'
import { useHostTerminal } from '../../components/use-host-terminal'
import { useStore } from '../../store/context'
import type { Backend } from '@shared/models'
import { market, type AuthMethod, type SetupStep } from './market-models'
import { ScreenHeader } from './pieces'
import { useTr } from '../../lib/use-language'

/**
 * Installing or signing in to an agent, or adding Node.js, live: the command runs in a
 * terminal on the computer and this screen is that terminal (links open here).
 */
export function SetupTerminalView({ backend, step, method, loginProfile, back }: {
  /** Null only for the `node` step, which belongs to the computer. */
  backend: Backend | null
  step: SetupStep
  /** A terminal sign-in method the agent offered; null runs Codync's own command. */
  method?: AuthMethod | null
  /** The login profile a sign-in is for; none signs in the computer's own login. */
  loginProfile?: string
  /** Back to the agent's sheet (the terminal is a step inside it). */
  back: () => void
}) {
  const tr = useTr()
  const store = useStore()
  const surface = useRef<HTMLDivElement>(null)
  const { exitCode, error } = useHostTerminal(surface, [backend?.id, step, method?.id].join('/'), {
    open: (client, cols, rows) => market.agentSetup(client, backend?.id ?? '', step, method?.id ?? null, cols, rows, loginProfile),
    // Leaving the screen ends whatever is still running.
    closeOnLeave: true,
    onExit: () => void store.refreshBackends(),
  })

  const name = backend?.name ?? tr('the agent')
  const title = step === 'node' ? 'Add Node.js' : step === 'install' ? `Install ${name}` : (method?.name ?? `Sign in to ${name}`)
  return (
    <SheetPage>
      <ScreenHeader title={title} onBack={back} />
      <div className="setup-terminal">
        <div ref={surface} className="setup-terminal-surface" />
      </div>
      <HStack gap={8} className="setup-terminal-status">
        {error ? (
          <Text tone="danger" lines={2}>{error}</Text>
        ) : exitCode !== null ? (
          <>
            <Icon name={exitCode === 0 ? 'checkmark.circle.fill' : 'exclamationmark.circle.fill'} size={10} color={exitCode === 0 ? 'var(--added)' : 'var(--warning)'} />
            <Text tone="secondary" lines={2}>{exitCode === 0 ? tr('Finished.') : tr('Ended with an error ({code}).').replaceAll('{code}', () => String(exitCode))}</Text>
          </>
        ) : (
          <>
            <ThinkingOrb size={14} color="var(--secondary)" />
            <Text tone="secondary" lines={2}>{tr('Running on {name}. Links open on this device.').replaceAll('{name}', () => store.hostName)}</Text>
          </>
        )}
        <Spacer />
        {exitCode !== null ? <Button className="fade-in" onClick={back}>Done</Button> : null}
      </HStack>
    </SheetPage>
  )
}
