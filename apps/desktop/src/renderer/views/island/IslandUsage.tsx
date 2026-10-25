import type { UsageProvider, UsageWindow } from '@shared/models'
import { prefs, usePref } from '../../lib/prefs'
import { usePixelRatio } from '../../lib/theme'
import { useResetLine, useTr } from '../../lib/use-language'
import { UsageBar } from '../usage/UsageBar'
import { mascotDataURL } from '../usage/ProviderMascot'
import { providerTintCSS, resetDescription, tightest, windowReading, windowTitle } from '../usage/usage-model'
import type { IslandModel } from './island-model'

// Codync's own part of the island: the plan limits the menu bar already tracks (local usage
// only), as a meter in the header and a Usage tab.

/** A provider's character for the black island, whatever the system appearance. */
function Mascot({ provider, size }: { provider: UsageProvider; size: number }) {
  const [style] = usePref(prefs.usageIconStyle)
  const ratio = usePixelRatio()
  const src = mascotDataURL(provider, size, style, true, ratio)
  return src ? <img src={src} alt="" width={size} height={size} className="island-mascot" /> : null
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
  const tr = useTr()
  const line = useResetLine()
  const title = tr(windowTitle(window))
  const reset = resetDescription(window)
  const spoken = reset ? line(reset) : ''
  return (
    <button
      className="island-meter"
      title={`${provider.name} · ${title} · ${Math.round(window.percent)}%${spoken ? ` · ${spoken}` : ''}`}
      aria-label={`Usage: ${provider.name} ${title} ${Math.round(window.percent)} percent`}
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

/**
 * The two providers closest to a limit side by side, each window a bar with when it resets. The
 * header meter's provider is always one of them; ones with notes only come last.
 */
export function UsageView({ model }: { model: IslandModel }) {
  const usage = [...model.summary.usage].sort((a, b) => (tightest(b)?.percent ?? -1) - (tightest(a)?.percent ?? -1))
  const tr = useTr()
  const line = useResetLine()
  if (!usage.length) {
    return (
      <div className="island-card island-empty">
        <span className="island-empty-text">
          <span className="island-title">No usage yet.</span>
          <span className="island-subtitle">{tr('Plan limits show up here once an agent reports them.')}</span>
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
              <span className="island-usage-title">{tr(windowTitle(w))}</span>
              {w.text ? (
                <span className="island-usage-note">{w.text}</span>
              ) : (
                <>
                  <UsageBar percent={w.percent} tint={providerTintCSS(provider.id)} />
                  <span className="island-usage-value">{windowReading(w)}</span>
                  <span className="island-usage-reset">{line(resetDescription(w) ?? '')}</span>
                </>
              )}
            </div>
          ))}
        </div>
      ))}
    </div>
  )
}
