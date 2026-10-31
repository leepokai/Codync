// Codync Cloud, as the apps show it: a computer Codync runs for the account (cloud/src/routes/cloud-computers.ts).

export type CloudState = 'starting' | 'running' | 'stopping' | 'stopped' | 'failed'

/** Where a cloud computer runs (`cloud` on an account computer). */
export interface CloudPlacement {
  provider: string
  providerName: string
  region: string
  state: CloudState
}

/** The account's cloud computer (`GET /v1/cloud-computers`); `computerId` stays null until it has claimed itself. */
export interface CloudMachine extends CloudPlacement {
  cloudComputerId: string
  computerId: string | null
  backupAt: number | null
  createdAt: number
}

export interface StatusLine {
  text: string
  error: boolean
}

/** "Codync Cloud · Boat · EU". */
export const placementLine = (p: CloudPlacement) => `Codync Cloud · ${p.providerName} · ${p.region}`

/** An offline cloud computer that sleeps isn't offline, and one that failed is an error; null: plain offline. */
export function offlineCloudStatus(cloud: CloudPlacement | null | undefined): StatusLine | null {
  if (!cloud) return null
  if (cloud.state === 'stopped' || cloud.state === 'stopping') return { text: 'Sleeping · wakes when you message', error: false }
  if (cloud.state === 'failed') return { text: 'Cloud computer failed', error: true }
  return null
}

/** Created but not claimed yet: asked again until its computer joins the account. */
export const isPreparing = (m: CloudMachine | null) => m !== null && m.computerId === null && m.state !== 'failed'

const stateNames: Record<CloudState, string> = { starting: 'Starting', running: 'Running', stopping: 'Going to sleep', stopped: 'Sleeping', failed: 'Failed' }

/** Settings → Codync Cloud's State. */
export function machineStatus(m: CloudMachine): StatusLine {
  if (isPreparing(m)) return { text: 'Preparing your cloud computer…', error: false }
  return { text: stateNames[m.state], error: m.state === 'failed' }
}
