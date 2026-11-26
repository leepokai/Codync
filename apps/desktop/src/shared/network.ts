/** This computer's networks of the user's computers (codync-network), as the host describes them. */

export type NetworkRole = 'owner' | 'admin' | 'member'

/** A computer in one of this computer's networks, as last heard. */
export interface NetworkComputer {
  name: string
  role: NetworkRole
  /** This computer. */
  me: boolean
  online: boolean
  /** The bots it offers, by address name and display name. */
  agents: { id: string; name: string }[]
  /** What this computer may do to it; the host decides. */
  canRemove: boolean
  canPromote: boolean
}

/** Who on a network's other computers may message this computer's bots; `*` matches any. The first match decides. */
export interface NetworkRule {
  fromComputer: string
  fromAgent: string
  toAgent: string
  /** What the sender may have a bot here do, the most trusted first; `auto` and `ask` are ceilings on how the bot works for it. */
  access: 'allow' | 'auto' | 'ask' | 'deny'
}

/** One network this computer is in. */
export interface Network {
  /** What the host calls it in `network` arguments. */
  id: string
  /** The network's own id, the same on every computer in it; absent on hosts before 2.29.0. */
  key?: string
  /** What the user calls it (`home`). */
  name: string
  /** This computer's name and role there. */
  computer: string
  role: NetworkRole
  /** What this computer may do there; the host decides. */
  canInvite: boolean
  canRecover: boolean
  /** `public`, `private` or a relay URL. */
  connector: string
  computers: NetworkComputer[]
  rules: NetworkRule[]
  /** What a computer no rule matches gets; absent on hosts before 2.49.0, which let it in at allow. */
  defaultAccess?: NetworkRule['access']
}

/** A coding agent's session on this computer with a Network address (`codync-host mcp network`). */
export interface NetworkSession {
  /** What bots and other agents message it at. */
  address: string
  /** The agent's own title for it; display only. */
  title?: string | null
  /** The agent (`claude-code`, `codex`…); a newer host may send one this app doesn't know. */
  kind: string
  /** The ACP registry id of its logo, when it has one. */
  registry?: string | null
  /** How it gets its mail: `pull` (it checks), later `turn` and `push`; unknown ones show nothing. */
  tier: string
  /** Whether its agent runs now; absent where nothing can tell. A newer host may send a state this app doesn't know. */
  state?: 'working' | 'idle' | 'online' | 'offline' | (string & {}) | null
  /** Running now, as far as the host can tell. */
  runs?: boolean
  /** The levels a rule to it can take, as the host decides (Allow and Deny). */
  levels?: NetworkRule['access'][]
  /** Folded under the older ones: neither running nor called in the last day. The host lists running ones first, then the most recent. */
  older?: boolean
  /** Its last call (Unix ms). */
  seen?: number
}

/** `networkView`, the `network` event and the stream's hello: this computer's networks, the default first, and cloud assistants. */
export interface NetworkView {
  joined: boolean
  networks: Network[]
  /** This computer's coding agents' sessions; absent on hosts before them. */
  sessions?: NetworkSession[]
  assistants: boolean
  /** Cloud assistants can reach this computer at all: it's in a Codync account. */
  assistantsAvailable: boolean
}

/** The recovery code of a network just made, kept until the user says they saved it. */
export interface RecoveryCode {
  code: string
  /** The network's name, and this computer's name in it. */
  network: string
  computer: string
}
