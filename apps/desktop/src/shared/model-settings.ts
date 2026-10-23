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

/** The settings to show for a bot's model (`''` is the agent's Default). */
export function modelSettings(catalog: Catalog | null | undefined, model: string): ModelSetting[] {
  return catalog?.models.find((m) => m.id === model)?.options ?? []
}
