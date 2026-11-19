import { useRef, useState, type ReactNode } from 'react'
import type { Usage, UsageProvider, UsageWindow } from '@shared/models'
import { IconButton, Reveal } from '../../components/Controls'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { prefs, usePref } from '../../lib/prefs'
import { useResetLine, useTr } from '../../lib/use-language'
import { ProviderMascot } from '../usage/ProviderMascot'
import { UsageBar } from '../usage/UsageBar'
import { logins, type UsageLogin } from '../usage/usage-logins'
import { providerTintCSS, resetDescription, windowTitle } from '../usage/usage-model'
import { relativeTime } from '../thread/chat-items'
import { UsageChooser } from './UsageChooser'
import './settings.css'

/** Every provider seen, in the order the host gives, so that one turned off can still fold away; and the ones the page opened with. */
function useProvidersSeen({ providers, services }: Usage) {
  const seen = useRef(new Map<string, UsageProvider>())
  const atFirst = useRef(new Set(providers.map((p) => p.id)))
  for (const provider of providers) seen.current.set(provider.id, provider)
  const rank = (id: string) => {
    const at = services?.findIndex((s) => s.id === id) ?? -1
    return at < 0 ? Number.MAX_SAFE_INTEGER : at
  }
  return { seen: [...seen.current.values()].sort((a, b) => rank(a.id) - rank(b.id)), atFirst: atFirst.current }
}

/** One computer's usage: its name, what its agents have used, and behind a button which agents show and in what order. */
export function UsageSection({ title, usage, onShown, onOrder }: {
  title: ReactNode
  usage: Usage
  onShown: (id: string, shown: boolean) => void
  onOrder: (ids: string[]) => Promise<void>
}) {
  const tr = useTr()
  const [editing, setEditing] = useState(false)
  const canEdit = !!usage.services?.length
  return (
    <VStack gap={10}>
      <HStack gap={6} className="settings-usage-computer">
        {title}
        <Spacer />
        {canEdit ? (
          <IconButton
            title={tr(editing ? 'Done' : 'Choose and arrange')}
            icon={editing ? 'checkmark' : 'slider.horizontal.3'}
            onClick={() => setEditing(!editing)}
          />
        ) : null}
      </HStack>
      {editing ? <UsageChooser usage={usage} onShown={onShown} onOrder={onOrder} /> : <UsageLimits usage={usage} />}
    </VStack>
  )
}

/** Every limit as a bar under its provider, the way the menu bar lists them. */
function UsageLimits({ usage }: { usage: Usage }) {
  const tr = useTr()
  const { seen, atFirst } = useProvidersSeen(usage)
  if (usage.providers.length === 0) return <Text tone="secondary">{tr('No usage information yet.')}</Text>
  return (
    <div className="settings-usage-card fade-in">
      {seen.map((provider) => (
        <Reveal key={provider.id} show={usage.providers.some((p) => p.id === provider.id)} animateOnMount={!atFirst.has(provider.id)}>
          <ProviderLimits provider={provider} />
        </Reveal>
      ))}
    </div>
  )
}

function StaleReading({ failedAt }: { failedAt: number }) {
  const tr = useTr()
  const when = relativeTime.day(failedAt, tr)
  return (
    <Text size="caption" tone="attention" className="settings-usage-stale" truncate>
      {tr('Out of date · check failed {when}').replaceAll('{when}', () => when)}
    </Text>
  )
}

function ProviderLimits({ provider }: { provider: UsageProvider }) {
  const [style] = usePref(prefs.usageIconStyle)
  const staleAt = !provider.loginProfiles?.length && provider.windows.length > 0 ? (provider.failedAt ?? null) : null
  return (
    <VStack gap={12} className="settings-usage-provider">
      <HStack gap={8}>
        <ProviderMascot provider={provider} size={18} style={style} />
        <Text size="body" weight="semibold" tone="text">{provider.name}</Text>
        {staleAt != null ? (
          <>
            <Spacer />
            <StaleReading failedAt={staleAt} />
          </>
        ) : null}
      </HStack>
      {provider.loginProfiles?.length
        ? logins(provider).map((login) => <LoginLimits key={login.id ?? ''} login={login} providerId={provider.id} />)
        : provider.windows.map((w) => <WindowLimit key={w.id} window={w} providerId={provider.id} stale={staleAt != null} />)}
    </VStack>
  )
}

/** One login of an agent that has several: what it's called, then its limits. */
function LoginLimits({ login, providerId }: { login: UsageLogin; providerId: string }) {
  const tr = useTr()
  const staleAt = login.windows.length > 0 ? (login.failedAt ?? null) : null
  return (
    <VStack gap={8} align="stretch" className="settings-usage-login">
      <HStack gap={8}>
        <Text tone="secondary" truncate>{login.name ?? tr('Default login profile')}</Text>
        {staleAt != null ? (
          <>
            <Spacer />
            <StaleReading failedAt={staleAt} />
          </>
        ) : null}
      </HStack>
      {login.windows.length
        ? login.windows.map((w) => <WindowLimit key={w.id} window={w} providerId={providerId} stale={staleAt != null} />)
        : <Text tone="secondary">{tr('No reading yet')}</Text>}
    </VStack>
  )
}

/** A limit's name and when it resets, how much is used, and under them the bar; or the note an agent gives instead. */
function WindowLimit({ window: w, providerId, stale = false }: { window: UsageWindow; providerId: string; stale?: boolean }) {
  const resetLine = useResetLine()
  const tr = useTr()
  return (
    <VStack gap={6} align="stretch" className={stale ? 'settings-usage-stale-windows' : undefined}>
      <HStack gap={8}>
        <Text tone="text" truncate>{tr(windowTitle(w))}</Text>
        {w.text ? (
          <Text tone="secondary" truncate grow className="settings-usage-note">{w.text}</Text>
        ) : (
          <>
            <Text tone="secondary" truncate grow>{resetLine(resetDescription(w) ?? '')}</Text>
            <Text tone="text" weight="medium" tabular>{Math.round(w.percent)}%</Text>
          </>
        )}
      </HStack>
      {w.text ? null : <UsageBar percent={w.percent} tint={providerTintCSS(providerId)} />}
    </VStack>
  )
}
