import { useState } from 'react'
import { fill } from '@shared/fill'
import type { NetworkSession } from '@shared/network'
import { AgentIcon } from '../../components/AgentIcon'
import { CardSection, Disclosure } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { sessionDotTitle, sessionStateTitle, tierTitle } from '../../lib/network-copy'
import { useTr } from '../../lib/use-language'

/**
 * This computer's coding agents' sessions with a Network address, in the host's order (running
 * first, then the most recent), the older ones folded under them.
 */
export function SessionsCard({ sessions }: { sessions: NetworkSession[] }) {
  const tr = useTr()
  const [showOlder, setShowOlder] = useState(false)
  const older = sessions.filter((s) => s.older)
  return (
    <CardSection title="Agents" footer="Bots and other agents message these sessions at their address; each reads its mail itself.">
      {sessions
        .filter((s) => !s.older)
        .map((s) => (
          <SessionRow key={s.address} session={s} />
        ))}
      {older.length ? (
        <Disclosure
          expanded={showOlder}
          onToggle={() => setShowOlder(!showOlder)}
          label={
            <Text size="callout" tone="secondary">
              {fill(tr('Older sessions: {count}'), { count: String(older.length) })}
            </Text>
          }
        >
          {older.map((s) => (
            <SessionRow key={s.address} session={s} />
          ))}
        </Disclosure>
      ) : null}
    </CardSection>
  )
}

/** Its title (else its address) by the agent's logo, then its address, whether it runs, and how it gets its mail. */
function SessionRow({ session: s }: { session: NetworkSession }) {
  const tr = useTr()
  const state = sessionStateTitle(s.state)
  const tier = tierTitle(s.tier)
  const detail = [s.title ? s.address : null, state ? tr(state) : null, tier ? tr(tier) : null].filter(Boolean).join(' · ')
  const dot = sessionDotTitle(s.state)
  return (
    <HStack gap={8} className="settings-row settings-appear">
      {/* No state, no dot: unknown isn't offline. */}
      <span
        className={`network-dot${s.runs ? ' online' : ''}${s.state ? '' : ' unknown'}`}
        role={dot ? 'img' : undefined}
        aria-label={dot ? tr(dot) : undefined}
        aria-hidden={dot ? undefined : true}
      />
      <AgentIcon registry={s.registry} size={16} />
      <VStack grow className="settings-stack">
        <Text truncate title={s.title ?? undefined}>{s.title ?? s.address}</Text>
        {detail ? <Text size="caption" tone="secondary" truncate>{detail}</Text> : null}
      </VStack>
    </HStack>
  )
}
