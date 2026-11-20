import { Icon } from './Icon'
import './agent-icon.css'

// Agents' logos by ACP registry id, or by Codync's id for agents outside the registry
// (template images: drawn in the current color).
const icons = import.meta.glob<string>('../assets/agents/*.svg', { eager: true, query: '?url', import: 'default' })
const byId = new Map(Object.entries(icons).map(([path, url]) => [path.split('/').pop()!.replace('.svg', ''), url]))

export function agentIconURL(registry: string | null | undefined) {
  return registry ? (byId.get(registry) ?? null) : null
}

/** Registry logos are one-color; paint the ones with a known brand color in it. */
export function agentTint(registry: string | null | undefined) {
  switch (registry) {
    case 'claude-acp': return 'rgb(217, 120, 87)'
    case 'gemini': return 'linear-gradient(to top right, rgb(71, 150, 227), rgb(145, 120, 199), rgb(201, 102, 115))'
    case 'antigravity-acp': return 'rgb(66, 133, 245)'
    case 'mistral-vibe': return 'rgb(250, 82, 15)'
    case 'qwen-code': return 'rgb(97, 92, 237)'
    case 'amp-acp': return 'rgb(242, 79, 64)'
    case 'kiro': return 'rgb(143, 69, 255)'
    case 'dsh': return 'rgb(77, 107, 254)'
    case 'cortex-code': return 'rgb(41, 181, 232)'
    case 'omp': return 'linear-gradient(to bottom right, rgb(237, 74, 191), rgb(155, 77, 255), rgb(90, 216, 230))'
    default: return 'var(--text)'
  }
}

/** An agent's logo from the ACP registry, or a generic terminal when it has none. `color` may be a gradient. */
export function AgentIcon({ registry, size, color = 'currentColor' }: { registry?: string | null; size: number; color?: string }) {
  const url = agentIconURL(registry)
  if (!url) return <Icon name="terminal" size={size * 0.8} scaled={false} color={color} />
  return <span aria-hidden className="agent-icon" style={{ width: size, height: size, background: color, WebkitMaskImage: `url("${url}")` }} />
}
