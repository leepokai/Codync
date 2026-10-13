import type { UsageProvider, UsageWindow } from '@shared/models'
import { prefs, usePref } from '../../lib/prefs'
import { UsageBar } from '../settings/UsageLimits'
import { mascotDataURL } from '../usage/ProviderMascot'
import { providerTintCSS, resetDescription, tightest, windowTitle } from '../usage/usage-model'
import type { IslandModel } from './island-model'

// Codync's own part of the island: the plan limits the menu bar already tracks (local usage
// only), as a meter in the header and a Usage tab.

/** A provider's character for the black island, whatever the system appearance. */
function Mascot({ provider, size }: { provider: UsageProvider; size: number }) {
  const [style] = usePref(prefs.usageIconStyle)
  const src = mascotDataURL(provider, size, style, true)
  return src ? <img src={src} alt="" width={size} height={size} style={{ flex: 'none' }} /> : null
}

/** The limit closest to running out, across providers. */
function tightestOverall(usage: UsageProvider[]): { provider: UsageProvider; window: UsageWindow } | null {
  let best: { provider: UsageProvider; window: UsageWindow } | null = null
  for (const provider of usage) {
    const window = tightest(provider)
    if (window && (!best || window.percent > best.window.percent)) best = { provider, window }
  }
  return best
}

/** Header: the tightest limit as its provider's character, a short bar and the percent; opens Usage. */
export function UsageMeter({ model }: { model: IslandModel }) {
  const top = tightestOverall(model.summary.usage)
  if (!top) return null
  const { provider, window } = top
  const reset = resetDescription(window)
  return (
    <button
      className="island-meter"
      title={`${provider.name} · ${windowTitle(window)} · ${Math.round(window.percent)}%${reset ? ` · ${reset}` : ''}`}
      aria-label={`Usage: ${provider.name} ${windowTitle(window)} ${Math.round(window.percent)} percent`}
      onClick={() => model.setTab('usage')}
    >
      <Mascot provider={provider} size={15} />
      <span className="island-meter-bar">
        <UsageBar percent={window.percent} tint={providerTintCSS(provider.id)} />
      </span>
      <span className="island-meter-value">{`${Math.round(window.percent)}%`}</span>
    </button>
  )
}

/** Every provider's limits side by side, each window a bar with when it resets. */
export function UsageView({ model }: { model: IslandModel }) {
  const usage = model.summary.usage
  if (!usage.length) {
    return (
      <div className="island-card island-empty">
        <span className="island-empty-text">
          <span className="island-title">No usage yet.</span>
          <span className="island-subtitle">Claude Code and Codex limits show up here once a bot has used them.</span>
        </span>
      </div>
    )
  }
  return (
    <div className="island-row-cards">
      {usage.slice(0, 2).map((provider) => (
        <div key={provider.id} className="island-card island-usage">
          <div className="island-agent">
            <Mascot provider={provider} size={20} />
            <span className="island-agent-name">{provider.name}</span>
          </div>
          {provider.windows.slice(0, 3).map((w) => (
            <div key={w.id} className="island-usage-row">
              <span className="island-usage-title">{windowTitle(w)}</span>
              <UsageBar percent={w.percent} tint={providerTintCSS(provider.id)} />
              <span className="island-usage-value">{`${Math.round(w.percent)}%`}</span>
              <span className="island-usage-reset">{resetDescription(w)?.replace('resets ', '') ?? ''}</span>
            </div>
          ))}
        </div>
      ))}
    </div>
  )
}
