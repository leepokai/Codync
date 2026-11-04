/** The computer's health as the host reports it (`hostHealth`), and the words every screen uses for it. */

export type HealthLevel = 'normal' | 'warning' | 'critical'

export interface HealthAgent {
  botId: string
  botName: string
  pid: number
  rss: number
}

export interface HostHealth {
  ramTotal: number
  ramFree: number
  ramUsedPercent: number
  cpu: number
  diskTotal: number
  diskFree: number
  level: HealthLevel
  /** New turns are not starting. */
  refusing: boolean
  thresholds: { warn: number; critical: number }
  agents: HealthAgent[]
  at: number
}

const GB = 1024 * 1024 * 1024
const MB = 1024 * 1024

/** `1.5 GB`, `320 MB`. */
export function bytesText(bytes: number): string {
  if (bytes >= GB) return `${(bytes / GB).toFixed(1)} GB`
  return `${Math.round(bytes / MB)} MB`
}

export function levelLabel(level: HealthLevel): string {
  switch (level) {
    case 'critical': return 'Memory is full'
    case 'warning': return 'Memory is getting full'
    default: return 'Healthy'
  }
}

/** One line for a small place (the island, the terminal's header): nothing when all is well. */
export function healthAlert(health: HostHealth | null): string | null {
  if (!health || health.level === 'normal') return null
  return health.refusing
    ? `Memory ${health.ramUsedPercent} % · not starting new turns`
    : `Memory ${health.ramUsedPercent} % · getting full`
}

/** The numbers, for the status tab. */
export function healthLines(health: HostHealth): { label: string; value: string }[] {
  return [
    { label: 'Memory', value: `${health.ramUsedPercent} % used · ${bytesText(health.ramFree)} free of ${bytesText(health.ramTotal)}` },
    { label: 'CPU', value: `${Math.round(health.cpu)} %` },
    { label: 'Disk', value: health.diskTotal > 0 ? `${bytesText(health.diskFree)} free of ${bytesText(health.diskTotal)}` : 'Unknown' },
  ]
}

/** The agents Codync started, the biggest first. */
export function agentRows(health: HostHealth): { key: string; name: string; use: string }[] {
  return [...health.agents]
    .sort((a, b) => b.rss - a.rss)
    .map((a) => ({ key: `${a.botId}:${a.pid}`, name: a.botName, use: bytesText(a.rss) }))
}

/** The pair the host would accept, or why not. */
export function thresholdProblem(warn: number, critical: number): string | null {
  if (!Number.isInteger(warn) || !Number.isInteger(critical)) return 'Use whole numbers.'
  if (warn < 1 || critical > 99) return 'Use values between 1 and 99 %.'
  if (warn >= critical) return 'The warning must come before the limit.'
  return null
}
