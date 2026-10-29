/** What the Network section says about this computer's networks (codync-network). */

import type { Network, NetworkComputer, NetworkRole, NetworkRule } from '@shared/network'

/** How computers reach each other without a direct path. */
export type ConnectorChoice = 'public' | 'private' | 'relay'

export const connectorChoices: { id: ConnectorChoice; label: string; detail: string }[] = [
  { id: 'public', label: 'Public relays', detail: 'Direct when it can, through a shared relay otherwise.' },
  { id: 'private', label: 'Private: direct only', detail: 'Same network, Tailscale or a VPN.' },
  { id: 'relay', label: 'Your own relay', detail: 'An iroh-relay you run.' },
]

/** The host's `connector` for a choice, or what's missing. An empty URL would quietly mean `public`. */
export function connectorFor(choice: ConnectorChoice, url: string): { connector: string } | { problem: string } {
  if (choice !== 'relay') return { connector: choice }
  const address = url.trim()
  return address ? { connector: address } : { problem: "Enter your relay's address." }
}

export const roleTitle = (role: NetworkRole) => ({ owner: 'Owner', admin: 'Admin', member: 'Member' })[role]

export const presenceTitle = (c: NetworkComputer) => (c.me ? 'This computer' : c.online ? 'Online' : 'Away')

/** "3 bots", untranslated template first: the caller translates, then puts the count in. */
export const botsTemplate = (c: NetworkComputer) => (c.agents.length === 1 ? '1 bot' : '{count} bots')

/** "3:27 PM" for an invite's expiry (Unix ms). */
export const expiryTime = (expiresAt: number) => new Date(expiresAt).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })

export const ANY = '*'

/** A computer of the network by name (names ignore case, like the host's). */
const computerNamed = (net: Network, name: string) => net.computers.find((c) => c.name.toLowerCase() === name.toLowerCase())

/** An agent's display name on a computer, else its address name. */
const agentName = (computer: NetworkComputer | undefined, id: string) => computer?.agents.find((a) => a.id === id)?.name ?? id

/** "Deny: any bot on laptop → Reviewer", through `tr` (templates translated before names go in). */
export function ruleLine(rule: NetworkRule, net: Network, tr: (s: string) => string): string {
  const computer = computerNamed(net, rule.fromComputer)
  const bot = agentName(computer, rule.fromAgent)
  const from =
    rule.fromComputer === ANY
      ? rule.fromAgent === ANY
        ? tr('any bot on any computer')
        : tr('{bot} on any computer').replaceAll('{bot}', bot)
      : rule.fromAgent === ANY
        ? tr('any bot on {computer}').replaceAll('{computer}', rule.fromComputer)
        : tr('{bot} on {computer}').replaceAll('{bot}', bot).replaceAll('{computer}', rule.fromComputer)
  const me = net.computers.find((c) => c.me)
  const to = rule.toAgent === ANY ? tr('any bot here') : agentName(me, rule.toAgent)
  const template = rule.access === 'allow' ? 'Allow: {from} → {to}' : 'Deny: {from} → {to}'
  return tr(template).replaceAll('{from}', from).replaceAll('{to}', to)
}

/** The rules with `i` moved by `by` (−1 up, +1 down); unchanged at either end. */
export function moved(rules: NetworkRule[], i: number, by: -1 | 1): NetworkRule[] {
  const j = i + by
  if (j < 0 || j >= rules.length) return rules
  const out = [...rules]
  ;[out[i], out[j]] = [out[j]!, out[i]!]
  return out
}
