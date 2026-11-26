import type { AgentMode } from './approvals'

/** A model's own setting (effort, fast…) and the values the agent accepts. */
export interface ModelOption {
  id: string
  name: string
  values: { id: string; name: string }[]
  default?: string | null
  kind?: string
}

/** Model identifiers and display names advertised by the selected computer's ACP agent. */
export interface AgentModels {
  models: { id: string; name: string; description?: string | null; options?: ModelOption[]; optionsError?: string }[]
  currentModelId?: string | null
  /** The agent offers Fast mode (absent on older hosts). */
  fastMode?: boolean
  defaultModelOptions?: boolean
  /** The agent's modes, its Auto mode first and listed as `auto` (absent on older hosts). */
  modes?: AgentMode[]
}
