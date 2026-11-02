/** Skills a bot proposes after a procedure that used several kinds of tool (`skillProposals`): they wait for a person. */

export interface SkillProposal {
  id: string
  botId: string
  name: string
  /** When to use it. */
  description: string
  /** The skill in Markdown. */
  instructions: string
  /** The kinds of tool the procedure used. */
  tools: string[]
  createdAt: number
}

/** The title of the inbox, with its count. */
export const inboxTitle = (count: number) => (count > 0 ? `Proposed skills (${count})` : 'Proposed skills')

/** What the tools were, for a card's footnote: `execute:cargo` reads as the program. */
export function toolsText(tools: readonly string[]): string {
  const words = tools.map((t) => (t.startsWith('execute:') ? t.slice('execute:'.length) || 'commands' : t))
  return words.join(', ')
}

/** The first steps of the instructions, for the card; the rest opens with Edit. */
export function stepsPreview(instructions: string, max = 3): string[] {
  return instructions.split('\n').filter((line) => /^\d+\.\s/.test(line)).slice(0, max)
}

/** A name and instructions that can be approved. */
export const approvable = (p: Pick<SkillProposal, 'name' | 'description' | 'instructions'>) =>
  p.name.trim() !== '' && p.description.trim() !== '' && p.instructions.trim() !== ''
