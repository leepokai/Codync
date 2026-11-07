// Codync Cloud, as the apps show it: a computer Codync runs for the account on the user's own Boat account, with the
// key they gave it (cloud/src/routes/cloud-computers.ts, cloud-credentials.ts).

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

/** The provider key the account gave, as the cloud shows it: never the key itself. */
export interface CloudCredential {
  provider: string
  providerName: string
  /** Its kind and last four, `boat_…9abc`. */
  hint: string
  /** What the owner has to fix while the provider turns the key down, in a sentence the app translates. */
  problem: string | null
}

/** How the user makes the key, worded by the cloud so it changes without an app release. */
export interface CloudKeySetup {
  providerName: string
  page: string
  commands: string
}

/** `cloud_key_saved`'s outcome for a save that threw: `refused` when the cloud or Boat turned the key down (the app
 * shows what to fix), `failed` for anything else. */
export const cloudKeyOutcome = (error: unknown): 'refused' | 'failed' =>
  error instanceof Error && 'code' in error && error.code === 'keyRefused' ? 'refused' : 'failed'

export interface StatusLine {
  text: string
  error: boolean
}

/** "Runs on Boat · EU", through `tr`: under the cloud computer's name, which is Codync Cloud already. */
export const placementLine = (p: CloudPlacement, tr: (text: string) => string) =>
  tr('Runs on {provider} · {region}').replace('{provider}', p.providerName).replace('{region}', p.region)

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

/** Computers → Codync Cloud's State. */
export function machineStatus(m: CloudMachine): StatusLine {
  if (isPreparing(m)) return { text: 'Preparing your cloud computer…', error: false }
  return { text: stateNames[m.state], error: m.state === 'failed' }
}
