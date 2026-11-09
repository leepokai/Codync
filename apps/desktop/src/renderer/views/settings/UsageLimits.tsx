import { useRef, useState, type ReactNode } from 'react'
import type { Usage, UsageProvider, UsageWindow } from '@shared/models'
import { IconButton, Reveal } from '../../components/Controls'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { prefs, usePref } from '../../lib/prefs'
import { useResetLine, useTr } from '../../lib/use-language'
import { ProviderMascot } from '../usage/ProviderMascot'
import { UsageBar } from '../usage/UsageBar'
import { providerTintCSS, resetDescription, windowTitle } from '../usage/usage-model'
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

function ProviderLimits({ provider }: { provider: UsageProvider }) {
  const [style] = usePref(prefs.usageIconStyle)
  return (
    <VStack gap={12} className="settings-usage-provider">
      <HStack gap={8}>
        <ProviderMascot provider={provider} size={18} style={style} />
        <Text size="body" weight="semibold" tone="text">{provider.name}</Text>
      </HStack>
      {provider.windows.map((w) => <WindowLimit key={w.id} window={w} providerId={provider.id} />)}
    </VStack>
  )
}

/** A limit's name and when it resets, how much is used, and under them the bar; or the note an agent gives instead. */
function WindowLimit({ window: w, providerId }: { window: UsageWindow; providerId: string }) {
  const resetLine = useResetLine()
  const tr = useTr()
  return (
    <VStack gap={6} align="stretch">
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
