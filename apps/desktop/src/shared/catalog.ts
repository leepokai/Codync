import type { BotDraft } from './models'

/** A ready-made bot from the host's catalog (`botCatalog`). */
export interface BotProfile {
  id: string
  name: string
  summary: string
  instructions: string
  avatarColor: string
  avatarShape: string
  /** Offered as a suggestion on the New bot screen. */
  featured: boolean
}

/** A group chat led by a coordinator bot, with the catalog bots it hands work to. */
export interface TeamProfile {
  id: string
  name: string
  summary: string
  purpose: string
  coordinator: BotProfile
  /** Ids of catalog bots. */
  members: string[]
}

export interface BotCatalog {
  bots: BotProfile[]
  teams: TeamProfile[]
  capabilities?: { roleModels?: boolean }
}

/** A draft that starts from `profile`: its name, look and instructions; the agent and model stay the draft's. */
export const fromProfile = (draft: BotDraft, profile: BotProfile): BotDraft => ({
  ...draft,
  name: profile.name,
  description: profile.instructions,
  avatarColor: profile.avatarColor,
  avatarShape: profile.avatarShape,
})

/** The coordinator, then the teammates the catalog has (an unknown id is skipped). */
export const teamProfiles = (catalog: BotCatalog, team: TeamProfile): BotProfile[] => [
  team.coordinator,
  ...team.members.flatMap((id) => catalog.bots.filter((b) => b.id === id)),
]
