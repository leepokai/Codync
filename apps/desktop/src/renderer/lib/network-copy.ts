/** What the Network section says about this computer's networks (codync-network). */

import type { Network, NetworkComputer, NetworkRole, NetworkRule, NetworkSession } from '@shared/network'
import { fill } from '../../shared/fill.ts'

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

/** What a running session's agent does, untranslated, where it says (working or idle); the dot alone shows online or offline. */
export const sessionStateTitle = (state: NetworkSession['state']) =>
  state ? (({ working: 'Working', idle: 'Idle' }) as Record<string, string>)[state] ?? null : null

/** What a session's dot says to a screen reader, untranslated, where no word shows (online or offline); null: no dot. */
export const sessionDotTitle = (state: NetworkSession['state']) =>
  state ? (({ online: 'Online', offline: 'Offline' }) as Record<string, string>)[state] ?? null : null

/** How a session gets its mail, untranslated; null for a tier this app doesn't know. */
export const tierTitle = (tier: string) => ({ pull: 'Checks its mail', turn: 'Told of mail at its next step', push: 'Told of mail as it comes' } as Record<string, string>)[tier] ?? null

/** The levels a rule to `session` can take, as the host sends them; to anyone else, every level. */
export const levelsFor = (session?: NetworkSession): NetworkRule['access'][] =>
  session ? (session.levels ?? ['allow', 'deny']) : ['allow', 'auto', 'ask', 'deny']

/** The rules' footer, untranslated: what decides, and what a computer no rule matches gets. */
export const rulesFooter = (net: Network) =>
  (net.defaultAccess ?? 'allow') === 'allow'
    ? "The first rule that matches decides. Without rules, every computer in the network may message every bot here. Answers to this computer's own messages always get in."
    : "The first rule that matches decides. A computer no rule matches gets Auto. Answers to this computer's own messages always get in."

const RULE_LINES: Record<NetworkRule['access'], string> = {
  allow: 'Allow: {from} → {to}',
  auto: 'Auto: {from} → {to}',
  ask: 'Ask: {from} → {to}',
  deny: 'Deny: {from} → {to}',
}

/** Whether saving rules failed because the computer's Codync is from before `auto` and `ask`: it refuses the whole
 *  list in its parser's words, which don't say that it needs updating. */
export const beforeCeilings = (problem: string) => /unknown variant `(auto|ask)`/.test(problem)

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
        : fill(tr('{bot} on any computer'), { bot })
      : rule.fromAgent === ANY
        ? fill(tr('any bot on {computer}'), { computer: rule.fromComputer })
        : fill(tr('{bot} on {computer}'), { bot, computer: rule.fromComputer })
  const me = net.computers.find((c) => c.me)
  const to = rule.toAgent === ANY ? tr('any bot here') : agentName(me, rule.toAgent)
  // An access this app doesn't know reads as deny, never wider than it is.
  return fill(tr(RULE_LINES[rule.access] ?? RULE_LINES.deny), { from, to })
}

/** The rules with `i` moved by `by` (−1 up, +1 down); unchanged at either end. */
export function moved(rules: NetworkRule[], i: number, by: -1 | 1): NetworkRule[] {
  const j = i + by
  if (j < 0 || j >= rules.length) return rules
  const out = [...rules]
  ;[out[i], out[j]] = [out[j]!, out[i]!]
  return out
}
