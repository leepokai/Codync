import { Icon } from './Icon'
import './agent-icon.css'

// Codync Cloud's providers' logos by the cloud's provider id (one-color, drawn like the agents' in `AgentIcon`).
const icons = import.meta.glob<string>('../assets/providers/*.svg', { eager: true, query: '?url', import: 'default' })
const byId = new Map(Object.entries(icons).map(([path, url]) => [path.split('/').pop()!.replace('.svg', ''), url]))

/** Boat in its brand blue in both themes, like the agents' tints; the others in the text color. */
const providerTint = (provider: string) => (provider === 'boat' ? 'rgb(38, 63, 169)' : 'var(--text)')

/** A cloud provider's logo, or the `cloud` symbol for one this app doesn't know yet. */
export function ProviderIcon({ provider, size }: { provider: string; size: number }) {
  const url = byId.get(provider)
  if (!url) return <Icon name="cloud" size={size * 0.8} scaled={false} color="var(--text)" />
  return <span aria-hidden className="agent-icon" style={{ width: size, height: size, background: providerTint(provider), WebkitMaskImage: `url("${url}")` }} />
}
