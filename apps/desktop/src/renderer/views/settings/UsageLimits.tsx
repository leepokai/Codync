import { useRef } from 'react'
import type { Usage, UsageProvider, UsageService } from '@shared/models'
import { Reveal, Switch } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { prefs, usePref } from '../../lib/prefs'
import { useResetLine, useTr } from '../../lib/use-language'
import { ProviderMascot } from '../usage/ProviderMascot'
import { UsageBar } from '../usage/UsageBar'
import { providerTintCSS, resetDescription, windowReading, windowTitle } from '../usage/usage-model'
import './settings.css'

/** What a mascot needs of an agent that has no reading. */
const unreadProvider = ({ id, name, registry }: UsageService): UsageProvider => ({ id, name, registry, windows: [], source: '', updatedAt: 0 })

/** Every provider seen, so that one turned off can still fold away, and the ones the page opened with. */
function useProvidersSeen(providers: UsageProvider[]) {
  const seen = useRef(new Map<string, UsageProvider>())
  const atFirst = useRef(new Set(providers.map((p) => p.id)))
  for (const provider of providers) seen.current.set(provider.id, provider)
  return { seen: [...seen.current.values()].sort((a, b) => a.id.localeCompare(b.id)), atFirst: atFirst.current }
}

/** Every limit as a bar under its provider, the way the menu bar lists them. */
export function UsageLimits({ usage }: { usage: Usage }) {
  const tr = useTr()
  const { seen, atFirst } = useProvidersSeen(usage.providers)
  return (
    <VStack gap={18}>
      {usage.providers.length === 0 ? <Text tone="secondary">{tr('No usage information yet.')}</Text> : null}
      {seen.map((provider) => (
        <Reveal key={provider.id} show={usage.providers.some((p) => p.id === provider.id)} animateOnMount={!atFirst.has(provider.id)}>
          <ProviderLimits provider={provider} />
        </Reveal>
      ))}
    </VStack>
  )
}

function ProviderLimits({ provider }: { provider: UsageProvider }) {
  const [style] = usePref(prefs.usageIconStyle)
  const resetLine = useResetLine()
  const tr = useTr()
  return (
    <VStack gap={10} className="settings-usage-provider">
      <HStack gap={8}>
        <ProviderMascot provider={provider} size={18} style={style} />
        <Text size="body" weight="semibold" tone="text">{provider.name}</Text>
      </HStack>
      {provider.windows.map((w) => (
        <HStack key={w.id} gap={10} className="settings-usage-row">
          <Text tone="text" truncate className="settings-usage-title">{tr(windowTitle(w))}</Text>
          {w.text ? (
            <Text tone="text" truncate className="settings-usage-note">{w.text}</Text>
          ) : (
            <>
              <UsageBar percent={w.percent} tint={providerTintCSS(provider.id)} />
              <Text tone="text" tabular className="settings-usage-percent">{windowReading(w, tr)}</Text>
              <Text tone="secondary" truncate className="settings-usage-reset">{resetLine(resetDescription(w) ?? '')}</Text>
            </>
          )}
        </HStack>
      ))}
    </VStack>
  )
}

/** Which agents' limits this computer lists. */
export function UsageChooser({ usage, onChange }: { usage: Usage; onChange: (id: string, shown: boolean) => void }) {
  const [style] = usePref(prefs.usageIconStyle)
  const tr = useTr()
  return (
    <VStack gap={10} className="settings-usage-chooser">
      <Text tone="secondary">{tr('Show usage for')}</Text>
      {usage.services?.map((service) => (
        <Switch key={service.id} on={service.shown} onChange={(shown) => onChange(service.id, shown)}>
          <HStack gap={8}>
            <ProviderMascot provider={unreadProvider(service)} size={18} style={style} />
            <Text tone="text" truncate>{service.name}</Text>
            {service.shown && !usage.providers.some((p) => p.id === service.id) ? (
              <Text tone="secondary" truncate>{tr('No reading yet')}</Text>
            ) : null}
          </HStack>
        </Switch>
      ))}
      <Text tone="secondary">{tr('On by default for the agents your bots run.')}</Text>
    </VStack>
  )
}
