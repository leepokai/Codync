import type { Usage } from '@shared/models'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { prefs, usePref } from '../../lib/prefs'
import { useResetLine } from '../../lib/use-language'
import { ProviderMascot } from '../usage/ProviderMascot'
import { UsageBar } from '../usage/UsageBar'
import { providerTintCSS, resetDescription, windowReading, windowTitle } from '../usage/usage-model'
import './settings.css'

/** Every limit as a bar under its provider, the way the menu bar lists them. */
export function UsageLimits({ usage }: { usage: Usage }) {
  const [style] = usePref(prefs.usageIconStyle)
  const resetLine = useResetLine()
  return (
    <VStack gap={18}>
      {usage.providers.map((provider) => (
        <VStack key={provider.id} gap={10}>
          <HStack gap={8}>
            <ProviderMascot provider={provider} size={18} style={style} />
            <Text size="body" weight="semibold" tone="text">{provider.name}</Text>
          </HStack>
          {provider.windows.map((w) => (
            <HStack key={w.id} gap={10} className="settings-usage-row">
              <Text tone="text" truncate className="settings-usage-title">{windowTitle(w)}</Text>
              {w.text ? (
                <Text tone="text" truncate className="settings-usage-note">{w.text}</Text>
              ) : (
                <>
                  <UsageBar percent={w.percent} tint={providerTintCSS(provider.id)} />
                  <Text tone="text" tabular className="settings-usage-percent">{windowReading(w)}</Text>
                  <Text tone="secondary" truncate className="settings-usage-reset">{resetLine(resetDescription(w) ?? '')}</Text>
                </>
              )}
            </HStack>
          ))}
        </VStack>
      ))}
    </VStack>
  )
}
