/** A model's own setting (effort, fast…) and the values the agent accepts. */
export interface ModelSetting {
  id: string
  name: string
  values: { id: string; name: string }[]
  default?: string | null
}

interface Catalog {
  models: { id: string; options?: ModelSetting[] }[]
  currentModelId?: string | null
}

/** The settings to show for a bot's model. Default (`''`) shows those of the model the agent
 *  runs by default, which is the one a bot on Default gets. */
export function modelSettings(catalog: Catalog | null | undefined, model: string): ModelSetting[] {
  const id = model || catalog?.currentModelId
  return catalog?.models.find((m) => m.id === id)?.options ?? []
}

// Stubs: the effort level row isn't built yet.
export const effortSetting = (_settings: ModelSetting[]): ModelSetting | undefined => undefined
export const reportFor = (_bot: unknown, _step: number): { options: Record<string, string> | null } | null => null
export const effortRow = (_input: unknown): { kind: string; caption: string | null; setting?: ModelSetting } => ({ kind: 'none', caption: null })
