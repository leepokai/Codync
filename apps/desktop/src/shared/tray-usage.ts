import type { TrayImage, TraySymbol, TrayUsageAgent, TrayUsageComputer, TrayUsageLogin } from './ipc'

/** A row of the tray menu's usage limits. */
export type UsageRow =
  /** Text over the rows after it. */
  | { title: string }
  /** The line between two logins. */
  | { rule: true }
  /** A button: it opens `rows` when it has them, else the app. */
  | { label: string; image?: TrayImage | null; symbol?: TraySymbol; rows?: UsageRow[] }

/** An agent's limits under its name; with several logins, one row that opens each login's. */
function agentRows(agent: TrayUsageAgent): UsageRow[] {
  const lines = (login: TrayUsageLogin): UsageRow[] => login.lines.map((l) => ({ label: l.title, image: l.bar }))
  const row = { label: agent.name, image: agent.icon }
  if (agent.logins.length < 2) return [row, ...agent.logins.flatMap(lines)]
  const logins = agent.logins.flatMap((login, i): UsageRow[] => [...(i ? [{ rule: true as const }] : []), { title: login.name }, ...lines(login)])
  return [{ ...row, rows: logins }]
}

/**
 * The tray menu's usage limits, a level at a time. An unnamed computer (the app's only one) lists
 * its agents in the menu itself; named ones each open theirs, under `title`.
 */
export function usageRows(usage: TrayUsageComputer[], title: string): UsageRow[] {
  const rows = usage.flatMap((computer): UsageRow[] => {
    const agents = computer.agents.flatMap(agentRows)
    return computer.name === null ? agents : [{ label: computer.name, symbol: computer.symbol, rows: agents }]
  })
  return usage.some((computer) => computer.name !== null) ? [{ title }, ...rows] : rows
}
