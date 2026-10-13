import type { Usage } from '@shared/models'
import { font } from '../../lib/fonts'
import { prefs, usePref } from '../../lib/prefs'
import { ProviderMascot } from '../usage/ProviderMascot'
import { providerTintCSS, resetDescription, windowTitle } from '../usage/usage-model'

/** Every limit as a bar under its provider, the way the menu bar lists them. */
export function UsageLimits({ usage }: { usage: Usage }) {
  const [style] = usePref(prefs.usageIconStyle)
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 18 }}>
      {usage.providers.map((provider) => (
        <div key={provider.id} style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <ProviderMascot provider={provider} size={18} style={style} />
            <span style={{ ...font(13, 'semibold'), color: 'var(--text)' }}>{provider.name}</span>
          </div>
          {provider.windows.map((w) => (
            <div key={w.id} style={{ ...font(12), display: 'flex', alignItems: 'center', gap: 10, whiteSpace: 'nowrap' }}>
              <span style={{ width: 64, flex: 'none', overflow: 'hidden', textOverflow: 'ellipsis', color: 'var(--text)' }}>{windowTitle(w)}</span>
              <UsageBar percent={w.percent} tint={providerTintCSS(provider.id)} />
              <span style={{ width: 36, flex: 'none', textAlign: 'right', fontVariantNumeric: 'tabular-nums', color: 'var(--text)' }}>{Math.round(w.percent)}%</span>
              <span style={{ width: 130, flex: 'none', textAlign: 'right', overflow: 'hidden', textOverflow: 'ellipsis', color: 'var(--secondary)' }}>{resetDescription(w) ?? ''}</span>
            </div>
          ))}
        </div>
      ))}
    </div>
  )
}

/** A capsule filled to `percent`: the provider's tint below 70%, then amber, red from 90%. */
export function UsageBar({ percent, tint }: { percent: number; tint: string }) {
  const fill = percent >= 90 ? 'var(--danger)' : percent >= 70 ? 'var(--warning)' : tint
  return (
    <span style={{ flex: 1, minWidth: 0, height: 6, borderRadius: 3, background: 'var(--bubble-agent)', overflow: 'hidden' }}>
      <span style={{ display: 'block', height: '100%', borderRadius: 3, background: fill, width: `${Math.min(1, Math.max(0.02, percent / 100)) * 100}%`, transition: 'width var(--layout)' }} />
    </span>
  )
}
