import { useState } from 'react'
import type { Usage } from '@shared/models'
import { IconButton } from '../components/Controls'
import { Icon } from '../components/Icon'
import { HStack } from '../components/Stack'
import { Text } from '../components/Text'
import { prefs, usePref } from '../lib/prefs'
import { useResetLine, useTr } from '../lib/use-language'
import { ProviderMascot } from './usage/ProviderMascot'
import { UsageBar } from './usage/UsageBar'
import { providerTintCSS, resetDescription, usageStatus, windowReading, windowTitle } from './usage/usage-model'
import './sidebar-usage.css'

/**
 * The sidebar's Usage section: every connector's plan windows, always visible
 * under the roster. The same reading the island and Settings show, as a column.
 */
export function SidebarUsage({ usage, onRefresh }: { usage: Usage; onRefresh?: () => Promise<void> }) {
  const tr = useTr()
  const resetLine = useResetLine()
  const [style] = usePref(prefs.usageIconStyle)
  const [open, setOpen] = useState(false)
  const [refreshing, setRefreshing] = useState(false)
  const providers = usage.providers
  const refresh = async () => {
    if (refreshing || !onRefresh) return
    setRefreshing(true)
    try {
      await onRefresh()
    } finally {
      setRefreshing(false)
    }
  }
  return (
    <section className="sidebar-usage">
      <div className="sidebar-usage-toggle">
        <button type="button" className="sidebar-usage-toggle-main" aria-expanded={open} onClick={() => {
          // Opening reads the plans again: they move while the sidebar sits there.
          if (!open) void refresh()
          setOpen((o) => !o)
        }}>
          <Icon name="gauge.with.dots.needle.33percent" size={13} />
          <span className="sidebar-usage-name">{tr('Usage')}</span>
          {providers.length ? <span className="sidebar-usage-count">{providers.length}</span> : null}
          <Icon name={open ? 'chevron.up' : 'chevron.down'} size={11} className="sidebar-usage-chevron" />
        </button>
        <IconButton
          title={tr('Refresh usage')}
          icon="arrow.clockwise"
          size={24}
          disabled={refreshing}
          className={refreshing ? 'sidebar-usage-refresh is-busy' : 'sidebar-usage-refresh'}
          onClick={() => void refresh()}
        />
      </div>
      {open ? (
        providers.length === 0 ? (
          <Text tone="secondary" size="caption" className="sidebar-usage-empty">
            {tr('Plan limits appear when an agent reports them.')}
          </Text>
        ) : (
          <div className="sidebar-usage-list">
            {providers.map((provider) => (
              <div key={provider.id} className="sidebar-usage-provider">
                <HStack gap={6} align="center" className="sidebar-usage-head">
                  <ProviderMascot provider={provider} size={15} style={style} />
                  <Text size="callout" weight="medium" tone="text" truncate>{provider.name}</Text>
                </HStack>
                {usageStatus(provider) ? <Text size="caption" tone="secondary">{tr(usageStatus(provider)!)}</Text> : null}
                {provider.windows.map((w) => (
                  <div key={w.id} className="sidebar-usage-window">
                    <div className="sidebar-usage-window-head">
                      <Text tone="secondary" size="caption" truncate>{tr(windowTitle(w))}</Text>
                      <Text tone="text" size="caption" tabular>{windowReading(w, tr)}</Text>
                    </div>
                    {w.text ? null : (
                      <>
                        <UsageBar percent={w.percent} tint={providerTintCSS(provider.id)} />
                        {resetDescription(w) ? (
                          <Text tone="tertiary" size="caption2" truncate>{resetLine(resetDescription(w)!)}</Text>
                        ) : null}
                      </>
                    )}
                  </div>
                ))}
              </div>
            ))}
          </div>
        )
      ) : null}
    </section>
  )
}
