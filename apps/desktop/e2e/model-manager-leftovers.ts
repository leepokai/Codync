/**
 * The names `model-manager.spec.ts` gives the bots it creates; a run that died before its `finally`
 * left one behind. `model-manager-cleanup.spec.ts` deletes these and then polls with the same filter,
 * so the filter itself is pinned against those literal names by `model-manager-leftovers.test.ts`.
 */
export const LEFTOVER = /^(?:Bot manager e2e [AB]|Provider (?:custom|agent)) \d+$/
