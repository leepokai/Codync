// Codync Cloud, as the apps show it: the computers Codync runs for the account on the user's own accounts with cloud
// providers, with the keys they gave them (cloud/src/routes/cloud-computers.ts, cloud-credentials.ts).

export type CloudState = 'starting' | 'running' | 'stopping' | 'stopped' | 'failed'

/** Where a cloud computer runs (`cloud` on an account computer). */
export interface CloudPlacement {
  provider: string
  providerName: string
  region: string
  state: CloudState
}

/** One of the account's cloud computers (`GET /v1/cloud-computers`); `computerId` stays null until it has claimed itself. */
export interface CloudMachine extends CloudPlacement {
  cloudComputerId: string
  /** What the user named it (also its account computer's name). */
  name: string
  computerId: string | null
  backupAt: number | null
  /** Whether its provider keeps snapshots (earlier states, a backup) that *Delete snapshots* erases; older clouds don't
   * say, and they all did. */
  snapshots?: boolean
  createdAt: number
}

/** A key the account gave a provider, as the cloud shows it: never the key itself. */
export interface CloudCredential {
  /** Its kind and last four, `boat_…9abc`. */
  hint: string
  /** What the owner has to fix while the provider turns the key down, in a sentence the app translates. */
  problem: string | null
  /** The key itself was turned down (revoked, expired): nothing can be made with it until it's replaced. Other problems
   * (the plan, the environment) leave Create on, to try again once they're fixed there. */
  blocking?: boolean
}

/**
 * A provider the cloud can run the computer on, as it lists them (`providers` in `GET /v1/cloud-computers`): where it
 * runs, how to make its key (worded by the cloud, so a new provider needs no app release) and the account's key for it.
 */
export interface CloudProvider {
  provider: string
  name: string
  region: string
  /** What its keys start with, for the field's placeholder (`boat_…`). */
  keyPrompt: string
  setup: { page: string; commands: string }
  credential: CloudCredential | null
  /** Its own free plan, if it has one (older clouds don't say). */
  offer?: CloudProviderOffer
}

/** A provider's own free plan, in the cloud's words (the app translates `label` and `detail`), and its page. */
export interface CloudProviderOffer {
  label: string
  detail: string
  url: string
}

/** A provider's key stays while a cloud computer runs there: without it the cloud couldn't put that computer to sleep. */
export const canRemoveKey = (machines: CloudMachine[], provider: string) => !machines.some((m) => m.provider === provider)

/** `cloud_key_saved`'s outcome for a save that threw: `refused` when the cloud or the provider turned the key down (the app
 * shows what to fix), `failed` for anything else. */
export const cloudKeyOutcome = (error: unknown): 'refused' | 'failed' =>
  error instanceof Error && 'code' in error && error.code === 'keyRefused' ? 'refused' : 'failed'

export interface StatusLine {
  text: string
  error: boolean
}

/** "Runs on Boat · EU", through `tr`: under the computer's name. */
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
export const isPreparing = (m: CloudMachine) => m.computerId === null && m.state !== 'failed'

/** A machine that was preparing in `before` has claimed itself in `after`: the account's computers list it now. */
export const justClaimed = (before: CloudMachine[], after: CloudMachine[]) =>
  after.some((m) => m.computerId !== null && before.some((b) => b.cloudComputerId === m.cloudComputerId && b.computerId === null))

const stateNames: Record<CloudState, string> = { starting: 'Starting', running: 'Running', stopping: 'Going to sleep', stopped: 'Sleeping', failed: 'Failed' }

/** What a cloud computer's `⋯` menu offers: its provider's key sheet among them; deleting is confirmed first. */
export type MachineAction = 'rename' | 'key' | 'deleteSnapshots' | 'delete'

/** One action at a time: a computer's carries its id (`delete:cm_1`), a key's its provider (`saveKey:boat`). */
export type CloudComputerAction = 'create' | `${MachineAction}:${string}` | `saveKey:${string}` | `removeKey:${string}`

/** The cloud computer `action` works on, if it's one of a computer's. */
export const actionMachine = (action: CloudComputerAction | null) => action?.match(/^(?:rename|deleteSnapshots|delete):(.+)$/)?.[1] ?? null

/**
 * Where an action's error shows: under the key in that provider's key sheet (`key:boat`), in the New cloud computer
 * sheet (`create`), else on the Codync Cloud card (the lookup, a computer's actions).
 */
export function errorPlace(action: CloudComputerAction | null): 'card' | 'create' | `key:${string}` {
  if (action === 'create') return 'create'
  const key = action?.match(/^(?:saveKey|removeKey):(.+)$/)?.[1]
  return key ? `key:${key}` : 'card'
}

/** A cloud computer's `⋯` menu, its labels through `tr`: its provider's key (to replace it or see what to fix), and
 * *Delete snapshots* only when its provider keeps any, and not while it prepares (it has none yet). */
export function machineActions(m: CloudMachine, tr: (text: string) => string): { action: MachineAction; label: string; disabled?: boolean }[] {
  return [
    { action: 'rename', label: tr('Rename…') },
    { action: 'key', label: tr('{provider} API key').replace('{provider}', m.providerName) },
    ...(m.snapshots === false ? [] : [{ action: 'deleteSnapshots', label: tr('Delete snapshots'), disabled: isPreparing(m) } as const]),
    { action: 'delete', label: tr('Delete cloud computer') },
  ]
}

/** A cloud computer's state; preparing until it has claimed itself; what to fix while its provider turns the key down
 * (`problem`), since nothing wakes or sleeps it until then. */
export function machineStatus(m: CloudMachine, problem?: string | null): StatusLine {
  if (problem) return { text: problem, error: true }
  if (isPreparing(m)) return { text: 'Preparing your cloud computer…', error: false }
  return { text: stateNames[m.state], error: m.state === 'failed' }
}

/** A cloud computer's name as typed, cut to what the cloud keeps (64 UTF-16 units, as it counts them). */
export const limitCloudName = (typed: string) => typed.slice(0, 64)

/** A new cloud computer's name when the user types none: "Cloud computer", else "Cloud computer 2" (the first number free). */
export function suggestedName(machines: CloudMachine[], tr: (text: string) => string) {
  const taken = new Set(machines.map((m) => m.name))
  const first = tr('Cloud computer')
  if (!taken.has(first)) return first
  for (let n = 2; ; n++) {
    const name = tr('Cloud computer {n}').replace('{n}', String(n))
    if (!taken.has(name)) return name
  }
}

/** `cloud_computer_created`'s outcome for a create that threw: `refused` when the cloud answered what to fix (409, or a
 * key the provider turned down), `failed` for anything else. */
export const cloudCreateOutcome = (error: unknown): 'refused' | 'failed' =>
  error instanceof Error && (('status' in error && error.status === 409) || ('code' in error && error.code === 'keyRefused')) ? 'refused' : 'failed'

/** A key whose own fault it isn't when the provider says no: a computer can be made there (or tried again). */
const usable = (p: CloudProvider) => p.credential !== null && !p.credential.blocking

/** Create is on: the account may have them, nothing else runs, and there's room for another. */
export const canCreate = (available: boolean, busy: boolean, count: number, limit: number) => available && !busy && count < limit

/** Why New cloud computer can't make another: the account has as many as it may (the cloud's own 409, untranslated). */
export const limitNote = (count: number, limit: number) =>
  count >= limit ? `You can have up to ${limit} cloud computers. Delete one to make another.` : null

/** A provider's row in New cloud computer: whether a computer can be made there and the line under its name (what to
 * fix, in the danger tone; without a key, that it has a free plan when it does). */
export function providerOption(p: CloudProvider, tr: (text: string) => string): { usable: boolean; detail: StatusLine } {
  if (p.credential?.problem) return { usable: usable(p), detail: { text: tr(p.credential.problem), error: true } }
  const missing = tr(p.offer ? 'Free sandbox available' : 'Add a key first')
  return { usable: usable(p), detail: { text: `${p.region} · ${p.credential?.hint ?? missing}`, error: false } }
}

/** The provider New cloud computer creates on: the one picked while its key is usable, else the first usable one. One
 * picked before its key was added (or fixed) is chosen once it is. */
export function chosenProvider(providers: CloudProvider[], picked: string | null) {
  const options = providers.filter(usable)
  return (options.find((p) => p.provider === picked) ?? options[0])?.provider ?? null
}
