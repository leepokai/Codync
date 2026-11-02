import type { BotDraft } from './models'

/** Codync's id for the agent's Auto mode, whatever the agent calls it: new bots start there. */
export const AUTO_MODE = 'auto'

/** One of the modes an agent works in (Claude's Manual / Plan / Auto, Codex's Read-only / Auto review…). */
export interface AgentMode {
  id: string
  name: string
  description?: string | null
}

/** What an agent's model list says about its modes (`agentModels`); null while it loads. */
export interface ModeCatalog {
  modes?: AgentMode[] | null
}

/** A bot's Approvals choice. `note` is the agent's own words, untranslated; `detail` is Codync's, to translate. */
export interface ApprovalChoice {
  id: string
  label: string
  note: string | null
  detail: string
}

const ASK = 'ask'
const APPROVE = 'approve'
const ASKS = 'Anything the agent still asks about comes to you as an approval card.'
const modeId = (mode: string) => `mode:${mode}`
const ask: ApprovalChoice = { id: ASK, label: 'Ask me', note: null, detail: "You'll get an approval card and a notification whenever the agent asks." }

/**
 * The Approvals choices for a bot and the one its setting shows as: the agent's modes, its Auto mode first, then
 * Codync approving everything. An agent without modes offers Ask me instead; while its list loads (or from an older
 * host), Auto and Ask me. A bot without a mode (or in Auto on an agent without one) shows as Ask me, and one in a mode
 * the list lacks as that mode's id: never as more careful than it is.
 */
export function approvals(draft: Pick<BotDraft, 'permission' | 'mode'>, catalog: ModeCatalog | null) {
  const approve: ApprovalChoice = {
    id: APPROVE, label: 'Approve automatically', note: null,
    detail: "Tool requests are approved automatically. Your agent's own settings (like Claude Code's permission rules) still apply.",
  }
  const modes = catalog?.modes
  const choices: ApprovalChoice[] = !modes
    ? [{ id: modeId(AUTO_MODE), label: 'Auto', note: null, detail: "The agent decides what's safe and asks you about the rest." }, ask]
    : !modes.length ? [ask] : modes.map((m) => ({ id: modeId(m.id), label: m.name, note: m.description ?? null, detail: ASKS }))
  choices.push(approve)
  const find = (id: string) => choices.find((c) => c.id === id)
  const mode = draft.mode ?? null
  // Unlisted: Auto on an agent without it is its default, like no mode; any other mode is itself.
  const unlisted = mode && mode !== AUTO_MODE ? { id: modeId(mode), label: mode, note: null, detail: ASKS } : ask
  let selected = draft.permission === 'auto' ? approve : find(mode ? modeId(mode) : ASK) ?? find(unlisted.id)
  if (!selected) {
    selected = unlisted
    choices.splice(choices.length - 1, 0, selected)
  }
  return { choices, selected }
}

/** The draft with an Approvals choice picked. */
export function withApproval<D extends Pick<BotDraft, 'permission' | 'mode'>>(draft: D, id: string): D {
  if (id === APPROVE) return { ...draft, permission: 'auto', mode: null }
  if (id === ASK) return { ...draft, permission: 'ask', mode: null }
  return { ...draft, permission: 'ask', mode: id.slice('mode:'.length) }
}
