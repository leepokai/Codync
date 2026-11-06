// Security profiles (read-only, builder, operator, full): what the host offers to choose (`securityProfiles`) and how a
// client words what a profile sets. The host owns the meaning; see docs/features/approvals.md.
import type { ApprovalRule, RuleDecision } from './approval-rules'

export interface SecurityProfile {
  id: string
  title: string
  /** One sentence for the person choosing. */
  summary: string
  /** The Approvals setting the profile leaves. */
  permission: 'ask' | 'auto'
  /** The daily token limit it sets, when it sets one. */
  budget: number | null
  rules: { kind: string | null; decision: RuleDecision }[]
}

/** The rules a profile writes, as the rules editor lists them (a `*` pattern per tool kind). */
export function profileRules(profile: SecurityProfile): ApprovalRule[] {
  return profile.rules.map((rule) => ({ pattern: '*', kind: rule.kind, decision: rule.decision }))
}

const VERBS: [RuleDecision, string][] = [
  ['allow', 'Allows'],
  ['ask', 'Asks before'],
  ['deny', 'Refuses'],
]

/** The lines that say what choosing a profile sets: by decision, the tool kinds, then the Approvals setting and the limit. */
export function previewLines(profile: SecurityProfile): string[] {
  const lines = VERBS.flatMap(([decision, verb]) => {
    const kinds = profile.rules.filter((rule) => rule.decision === decision).map((rule) => rule.kind ?? 'anything')
    return kinds.length ? [`${verb}: ${kinds.join(', ')}`] : []
  })
  lines.push(
    profile.permission === 'auto'
      ? 'Everything else: approved automatically'
      : 'Everything else: asks you',
  )
  if (profile.budget !== null) lines.push(`Daily limit: ${profile.budget.toLocaleString('en-US')} tokens`)
  lines.push('Commands no bot may run stay refused under every profile.')
  return lines
}
