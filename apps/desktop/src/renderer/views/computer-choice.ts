/** A computer menu on a page or sheet: which computer it is on, and the others it can switch to. */
export interface ComputerChoice {
  options: { id: string; label: string }[]
  selection: string
  onChange: (id: string) => void
}

interface Computer { computer: { id: string }; hostName: string }

/** The menu over `stores` with `selection` picked; null when there is no other computer to pick. */
export function computerChoice(stores: Computer[], selection: string | undefined, onChange: (id: string) => void): ComputerChoice | null {
  return selection && stores.length > 1
    ? { options: stores.map((s) => ({ id: s.computer.id, label: s.hostName })), selection, onChange }
    : null
}

/** The first of `preferred` that is one of `stores`, else the first of `stores`. */
export function firstAmong<T>(stores: T[], preferred: (T | null | undefined)[]): T | null {
  return preferred.find((s): s is T => !!s && stores.includes(s)) ?? stores[0] ?? null
}
