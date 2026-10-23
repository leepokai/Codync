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
