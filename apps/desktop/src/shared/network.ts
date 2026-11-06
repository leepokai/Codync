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
  access: 'allow' | 'deny'
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
}

/** `networkView`, the `network` event and the stream's hello: this computer's networks, the default first, and cloud assistants. */
export interface NetworkView {
  joined: boolean
  networks: Network[]
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
