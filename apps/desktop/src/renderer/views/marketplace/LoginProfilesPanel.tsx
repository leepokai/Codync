import { useEffect, useRef, useState } from 'react'
import type { Backend } from '@shared/models'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { useTr } from '../../lib/use-language'
import { AgentBrings } from './AgentBrings'
import { AgentTile } from './AgentSheet'
import { LoginProfileKeys } from './LoginProfileKeys'
import { LoginProfiles } from './LoginProfiles'
import { errorText, market } from './market-models'
import { ErrorText, FormSheet, OptionRow } from './pieces'
import { SetupTerminalView } from './SetupTerminal'

/** What a login profile's sheet shows: its sign-in terminal (an account) or its key form (an API key). */
type ProfileRoute = { kind: 'signIn'; loginProfile: string } | { kind: 'keys'; loginProfile: string }

/**
 * The Login profiles tab of Extensions: every agent here that takes login profiles, each with its default login and
 * its others, where they are added, named, signed in, given keys and removed. `focus` scrolls to one agent's (from a
 * bot's "Add login profile…" or the model panel). The agent's own setup (install, its default sign-in) stays in its
 * sheet, which `onAgent` opens.
 */
export function LoginProfilesPanel({ agents, query, focus, onAgent }: {
  agents: Backend[]
  query: string
  focus?: string
  onAgent: (backend: Backend) => void
}) {
  const tr = useTr()
  const list = useRef<HTMLDivElement>(null)
  const shown = agents.filter((b) => b.loginProfiles !== undefined && (b.installed === true || b.available))
  useEffect(() => {
    if (focus) list.current?.querySelector(`[data-agent="${CSS.escape(focus)}"]`)?.scrollIntoView({ block: 'start' })
  }, [focus])
  return (
    <div ref={list} className="market-panel-scroll fade-in">
      {shown.length === 0 ? (
        <Text tone="secondary">{query ? tr('No agents match “{query}”.').replaceAll('{query}', query) : tr('No agent here takes login profiles yet.')}</Text>
      ) : (
        <div className="login-profiles-panel">
          <Text size="footnote" tone="secondary">{tr('A bot picks its login profile under Agent in its settings. Bots that pick none use the default one.')}</Text>
          {shown.map((b) => <AgentLogins key={b.id} backend={b} onAgent={() => onAgent(b)} />)}
        </div>
      )}
    </div>
  )
}

/** One agent's login profiles, with the sheets they open. */
function AgentLogins({ backend, onAgent }: { backend: Backend; onAgent: () => void }) {
  const store = useStore()
  const tr = useTr()
  const [route, setRoute] = useState<ProfileRoute | null>(null)
  const [checking, setChecking] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const signedIn = backend.signedIn ?? null
  const status = checking ? 'Checking…' : signedIn === true ? 'Signed in.' : signedIn === false ? 'Not signed in yet.' : `Couldn't tell. Skip this if you've already signed in on ${store.hostName}.`

  const check = async () => {
    const client = store.client
    if (!client) return
    setChecking(true)
    try {
      await market.agentAuth(client, backend.id)
      setError(null)
    } catch (e) {
      setError(errorText(e))
    }
    setChecking(false)
    await store.refreshBackends()
  }
  // Back from a sign-in terminal: ask the agent whether it went through, and as whom.
  const close = () => {
    if (route?.kind === 'signIn') {
      void store.checkLoginProfile(backend.id, route.loginProfile).catch((e: unknown) => setError(errorText(e)))
    }
    setRoute(null)
  }

  return (
    <div data-agent={backend.id} className="agent-logins">
      <LoginProfiles
        backend={backend}
        footer={null}
        title={(
          <HStack gap={10} align="center">
            <AgentTile registry={backend.registry ?? backend.id} icon={14} tile={24} radius={7} />
            <Text size="body" weight="semibold" tone="text">{backend.name}</Text>
          </HStack>
        )}
        own={{ status, checking, check: () => void check() }}
        signIn={(loginProfile) => setRoute({ kind: 'signIn', loginProfile })}
        editKeys={(loginProfile) => setRoute({ kind: 'keys', loginProfile })}
      >
        {/* The default login signs in where the agent is set up. */}
        {signedIn === true || checking ? null : (
          <OptionRow icon="person.crop.circle" title={tr('Sign in')} detail={tr('Opens the agent’s setup.')} onClick={onAgent} />
        )}
        {error ? <ErrorText text={error} /> : null}
      </LoginProfiles>
      {/* Read from the user's home. An account profile's folder links or copies that same setup before each start,
          and an API key profile runs on the home itself, so every login brings the same. */}
      <AgentBrings backendId={backend.id} title="Brought in by default" />
      <FormSheet open={route !== null} onClose={close} width={560} height={560} fixed>
        {route?.kind === 'signIn' ? <SetupTerminalView backend={backend} step="login" method={null} loginProfile={route.loginProfile} back={close} />
          : route?.kind === 'keys' ? <LoginProfileKeys backend={backend} loginProfile={route.loginProfile} back={close} />
          : null}
      </FormSheet>
    </div>
  )
}
