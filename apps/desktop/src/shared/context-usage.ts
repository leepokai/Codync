/** The current agent session's last measurement, calculated by the host (never billing totals). */
export interface ContextUsage {
  usedTokens: number
  maxTokens: number
  percent: number
  level: string
}
